import { expect } from 'chai';
import * as protobuf from 'protobufjs';
import * as zlib from 'zlib';
import fs from 'node:fs/promises';
import path from 'node:path';
import { ethers } from 'ethers';
import * as zksync from 'zksync-ethers';

import {
    getExternalNodeHealth,
    sleep,
    NodeComponents,
    NodeProcess,
    dropNodeData,
    executeCommandWithLogs,
    FundedWallet
} from '../src';
import {
    setChunkSize,
    setDataRetentionSec,
    setRemovalDelaySec,
    setSnapshotRecovery,
    setTreeRecoveryParallelPersistenceBuffer,
    readContract
} from './utils';
import { loadConfig, shouldLoadConfigFromFile } from 'utils/build/file-configs';
import { logsTestPath } from 'utils/build/logs';

const pathToHome = path.join(__dirname, '../../../..');
const fileConfig = shouldLoadConfigFromFile();

interface AllSnapshotsResponse {
    readonly snapshotsL1BatchNumbers: number[];
}

interface GetSnapshotResponse {
    readonly version: number;
    readonly miniblockNumber: number;
    readonly l1BatchNumber: number;
    readonly storageLogsChunks: Array<StorageLogChunkMetadata>;
    readonly factoryDepsFilepath: string;
}

interface StorageLogChunkMetadata {
    readonly filepath: string;
}

interface FactoryDependencies {
    readonly factoryDeps: Array<FactoryDependency>;
}

interface FactoryDependency {
    readonly bytecode: Buffer;
    readonly hash?: Buffer;
}

interface StorageLogChunk {
    readonly storageLogs: Array<StorageLog>;
}

interface StorageLog {
    readonly accountAddress: Buffer;
    readonly storageKey: Buffer;
    readonly storageValue: Buffer;
    readonly l1BatchNumberOfInitialWrite: number;
    readonly enumerationIndex: number;
}

interface TokenInfo {
    readonly l1_address: string;
    readonly l2_address: string;
}

async function logsPath(name: string): Promise<string> {
    return await logsTestPath(fileConfig.chain, 'logs/recovery/snapshot/', name);
}

/**
 * Tests snapshot recovery and node state pruning.
 *
 * Assumptions:
 *
 * - Main node is run for the duration of the test.
 * - "Rich wallet" 0x36615Cf349d7F6344891B1e7CA7C72883F5dc049 is funded on L1. This is always true if the environment
 *   was initialized via `zk init`.
 * - `ZKSYNC_ENV` variable is not set (checked at the start of the test). For this reason,
 *   the test doesn't have a `zk` wrapper; it should be launched using `yarn`.
 */
describe('snapshot recovery', () => {
    const STORAGE_LOG_SAMPLE_PROBABILITY = 0.1;
    // Number of L1 batches awaited to be pruned.
    const PRUNED_BATCH_COUNT = 1;

    const homeDir = process.env.ZKSYNC_HOME!!;

    const disableTreeDuringPruning = process.env.DISABLE_TREE_DURING_PRUNING === 'true';
    console.log(`Tree is ${disableTreeDuringPruning ? 'disabled' : 'enabled'} during pruning`);

    const externalNodeEnvProfile =
        'ext-node' +
        (process.env.DEPLOYMENT_MODE === 'Validium' ? '-validium' : '') +
        (process.env.IN_DOCKER ? '-docker' : '');
    console.log('Using external node env profile', externalNodeEnvProfile);
    let externalNodeEnv: { [key: string]: string } = {
        ...process.env,
        ZKSYNC_ENV: externalNodeEnvProfile,
        EN_SNAPSHOTS_RECOVERY_ENABLED: 'true',
        // Test parallel persistence for tree recovery, which is (yet) not enabled by default
        EN_EXPERIMENTAL_SNAPSHOTS_RECOVERY_TREE_PARALLEL_PERSISTENCE_BUFFER: '4'
    };

    const autoKill: boolean = !fileConfig.loadFromFile || !process.env.NO_KILL;

    let snapshotMetadata: GetSnapshotResponse;
    let mainNode: zksync.Provider;
    let externalNode: zksync.Provider;
    let externalNodeProcess: NodeProcess;

    let fundedWallet: FundedWallet;
    let erc20Abi: ethers.InterfaceAbi;
    let erc20Address: string | undefined = undefined;

    let apiWeb3JsonRpcHttpUrl: string;
    let ethRpcUrl: string;
    let externalNodeUrl: string;
    let extNodeHealthUrl: string;

    before('prepare environment', async () => {
        expect(process.env.ZKSYNC_ENV, '`ZKSYNC_ENV` should not be set to allow running both server and EN components')
            .to.be.undefined;

        if (fileConfig.loadFromFile) {
            const secretsConfig = loadConfig({ pathToHome, chain: fileConfig.chain, config: 'secrets.yaml' });
            const generalConfig = loadConfig({ pathToHome, chain: fileConfig.chain, config: 'general.yaml' });
            const externalNodeGeneralConfig = loadConfig({
                pathToHome,
                chain: fileConfig.chain,
                configsFolderSuffix: 'external_node',
                config: 'general.yaml'
            });

            ethRpcUrl = secretsConfig.l1.l1_rpc_url;
            apiWeb3JsonRpcHttpUrl = generalConfig.api.web3_json_rpc.http_url;
            externalNodeUrl = externalNodeGeneralConfig.api.web3_json_rpc.http_url;
            extNodeHealthUrl = `http://127.0.0.1:${externalNodeGeneralConfig.api.healthcheck.port}/health`;

            setSnapshotRecovery(pathToHome, fileConfig, true);
            setTreeRecoveryParallelPersistenceBuffer(pathToHome, fileConfig, 4);
        } else {
            ethRpcUrl = process.env.ETH_CLIENT_WEB3_URL ?? 'http://127.0.0.1:8545';
            apiWeb3JsonRpcHttpUrl = 'http://127.0.0.1:3050';
            externalNodeUrl = 'http://127.0.0.1:3060';
            extNodeHealthUrl = 'http://127.0.0.1:3081/health';
        }

        mainNode = new zksync.Provider(apiWeb3JsonRpcHttpUrl);
        externalNode = new zksync.Provider(externalNodeUrl);
        if (autoKill) {
            await NodeProcess.stopAll('KILL');
        }
    });

    before('create test wallet', async () => {
        console.log(`Using L1 RPC at ${ethRpcUrl}`);
        const eth = new ethers.JsonRpcProvider(ethRpcUrl);
        fundedWallet = await FundedWallet.create(mainNode, eth);
    });

    after(async () => {
        if (externalNodeProcess) {
            await externalNodeProcess.stopAndWait('KILL');
            await externalNodeProcess.logs.close();
        }

        if (fileConfig.loadFromFile) {
            setSnapshotRecovery(pathToHome, fileConfig, false);
            setChunkSize(pathToHome, fileConfig, 10);
            setDataRetentionSec(pathToHome, fileConfig, 3600);
            setRemovalDelaySec(pathToHome, fileConfig, 60);
            setTreeRecoveryParallelPersistenceBuffer(pathToHome, fileConfig, 1);
        }
    });

    async function getAllSnapshots() {
        const output = await mainNode.send('snapshots_getAllSnapshots', []);
        return output as AllSnapshotsResponse;
    }

    async function getSnapshot(snapshotL1Batch: number) {
        const output = await mainNode.send('snapshots_getSnapshot', [snapshotL1Batch]);
        return output as GetSnapshotResponse;
    }

    async function getAllTokens(provider: zksync.Provider, atBlock?: number) {
        const output = await provider.send('en_syncTokens', atBlock ? [atBlock] : []);
        return output as TokenInfo[];
    }

    step('deploy EVM bytecode if allowed', async () => {
        const systemContractsPath = '../../../contracts/system-contracts/zkout';
        const contractDeployerAbi = readContract(systemContractsPath, 'ContractDeployer').abi;
        const contractDeployer = new zksync.Contract(
            '0x0000000000000000000000000000000000008006',
            contractDeployerAbi,
            mainNode
        );
        const allowedBytecodeTypes = await contractDeployer.allowedBytecodeTypesToDeploy();
        console.log('Allowed bytecode types', allowedBytecodeTypes);

        if (allowedBytecodeTypes === 1n) {
            console.log('Deploying EVM contract...');
            const l1ContractsPath = '../../../contracts/l1-contracts/out';
            const erc20Contract = readContract(l1ContractsPath, 'ERC20');
            erc20Abi = erc20Contract.abi;
            const erc20Factory = new ethers.ContractFactory(erc20Abi, erc20Contract.bytecode, fundedWallet.wallet);
            const erc20 = await (await erc20Factory.deploy('test', 'TEST')).waitForDeployment();
            erc20Address = await erc20.getAddress();
            console.log('Deployed EVM contract', erc20Address);

            const symbol = await erc20.getFunction('symbol').staticCall();
            expect(symbol).to.equal('TEST');

            // Ensure that the contract is included into the snapshot. The first call may seal the batch with the deployment transaction,
            // and we need one more batch.
            await fundedWallet.generateL1Batch();
            await fundedWallet.generateL1Batch();
        } else {
            console.log('EVM contracts are disabled');
        }
    });

    step('create snapshot', async () => {
        await createSnapshot(fileConfig.loadFromFile);
    });

    step('validate snapshot', async () => {
        const allSnapshots = await getAllSnapshots();
        console.log('Obtained all snapshots', allSnapshots);
        const newBatchNumbers = allSnapshots.snapshotsL1BatchNumbers;

        const l1BatchNumber = Math.max(...newBatchNumbers);
        snapshotMetadata = await getSnapshot(l1BatchNumber);
        console.log('Obtained latest snapshot', snapshotMetadata);
        expect(snapshotMetadata.version).to.be.oneOf([0, 1]);
        const l2BlockNumber = snapshotMetadata.miniblockNumber;

        const protoPath = path.join(homeDir, 'core/lib/types/src/proto/mod.proto');
        const root = await protobuf.load(protoPath);
        const SnapshotFactoryDependencies = root.lookupType('zksync.types.SnapshotFactoryDependencies');
        const SnapshotStorageLogsChunk = root.lookupType('zksync.types.SnapshotStorageLogsChunk');

        const factoryDepsPath = path.join(homeDir, snapshotMetadata.factoryDepsFilepath);
        console.log('Checking factory deps', factoryDepsPath);
        const output = SnapshotFactoryDependencies.decode(
            await decompressGzip(factoryDepsPath)
        ) as any as FactoryDependencies;
        expect(output.factoryDeps.length).to.be.greaterThan(0);
        let solidityBytecodeCount = 0;
        for (const dep of output.factoryDeps) {
            expect(dep.hash).to.have.length(32);
            if (dep.bytecode[0] === 0x60 && dep.bytecode[1] === 0x80) {
                console.log('Discovered Solidity bytecode', dep.hash);
                solidityBytecodeCount++;
            }
        }
        if (erc20Address !== undefined) {
            expect(solidityBytecodeCount).to.be.greaterThan(0);
        }

        expect(snapshotMetadata.l1BatchNumber).to.equal(l1BatchNumber);
        for (const chunkMetadata of snapshotMetadata.storageLogsChunks) {
            const chunkPath = path.join(homeDir, chunkMetadata.filepath);
            console.log(`Checking storage logs chunk ${chunkPath}`);
            const output = SnapshotStorageLogsChunk.decode(await decompressGzip(chunkPath)) as any as StorageLogChunk;
            expect(output.storageLogs.length).to.be.greaterThan(0);
            console.log(`Decompressed chunk has ${output.storageLogs.length} logs`);

            let sampledCount = 0;
            for (const storageLog of output.storageLogs) {
                // Randomly sample logs to speed up the test.
                if (Math.random() > STORAGE_LOG_SAMPLE_PROBABILITY) {
                    continue;
                }
                sampledCount++;

                const snapshotL1BatchNumber = storageLog.l1BatchNumberOfInitialWrite;
                expect(snapshotL1BatchNumber).to.be.lessThanOrEqual(l1BatchNumber);

                if (snapshotMetadata.version === 0) {
                    const snapshotAccountAddress = '0x' + storageLog.accountAddress.toString('hex');
                    const snapshotKey = '0x' + storageLog.storageKey.toString('hex');
                    const snapshotValue = '0x' + storageLog.storageValue.toString('hex');
                    const valueOnBlockchain = await mainNode.getStorage(
                        snapshotAccountAddress,
                        snapshotKey,
                        l2BlockNumber
                    );
                    expect(snapshotValue).to.equal(valueOnBlockchain);
                }
            }
            console.log(`Checked random ${sampledCount} logs in the chunk`);
        }
    });

    step('drop external node data', async () => {
        await dropNodeData(externalNodeEnv, fileConfig.loadFromFile, fileConfig.chain);
    });

    step('initialize external node', async () => {
        externalNodeProcess = await NodeProcess.spawn(
            externalNodeEnv,
            await logsPath('external_node.log'),
            pathToHome,
            NodeComponents.STANDARD,
            fileConfig.loadFromFile,
            fileConfig.chain
        );

        let recoveryFinished = false;
        let consistencyCheckerSucceeded = false;
        let reorgDetectorSucceeded = false;

        while (!recoveryFinished || !consistencyCheckerSucceeded || !reorgDetectorSucceeded) {
            await sleep(1000);
            const health = await getExternalNodeHealth(extNodeHealthUrl);
            if (health === null) {
                continue;
            }

            if (!recoveryFinished) {
                const status = health.components.snapshot_recovery?.status;
                expect(status).to.be.oneOf([undefined, 'affected', 'ready']);
                const details = health.components.snapshot_recovery?.details;
                if (details !== undefined) {
                    console.log('Received snapshot recovery health details', details);
                    expect(details.snapshot_l1_batch).to.equal(snapshotMetadata.l1BatchNumber);
                    expect(details.snapshot_l2_block).to.equal(snapshotMetadata.miniblockNumber);

                    if (
                        details.factory_deps_recovered &&
                        details.tokens_recovered &&
                        details.storage_logs_chunks_left_to_process === 0
                    ) {
                        console.log('Snapshot recovery is complete');
                        recoveryFinished = true;
                    }
                }
            }

            if (!consistencyCheckerSucceeded) {
                const status = health.components.consistency_checker?.status;
                expect(status).to.be.oneOf([undefined, 'not_ready', 'ready']);
                const details = health.components.consistency_checker?.details;
                if (status === 'ready' && details !== undefined) {
                    console.log('Received consistency checker health details', details);
                    if (details.first_checked_batch !== undefined && details.last_checked_batch !== undefined) {
                        expect(details.first_checked_batch).to.equal(snapshotMetadata.l1BatchNumber + 1);
                        expect(details.last_checked_batch).to.be.greaterThan(snapshotMetadata.l1BatchNumber);
                        consistencyCheckerSucceeded = true;
                    }
                }
            }

            if (!reorgDetectorSucceeded) {
                const status = health.components.reorg_detector?.status;
                expect(status).to.be.oneOf([undefined, 'not_ready', 'ready']);
                const details = health.components.reorg_detector?.details;
                if (status === 'ready' && details !== undefined) {
                    console.log('Received reorg detector health details', details);
                    if (details.last_correct_l1_batch !== undefined && details.last_correct_l2_block !== undefined) {
                        expect(details.last_correct_l1_batch).to.be.greaterThan(snapshotMetadata.l1BatchNumber);
                        expect(details.last_correct_l2_block).to.be.greaterThan(snapshotMetadata.miniblockNumber);
                        reorgDetectorSucceeded = true;
                    }
                }
            }
        }

        // If `externalNodeProcess` fails early, we'll trip these checks.
        expect(externalNodeProcess.exitCode()).to.be.null;
        expect(consistencyCheckerSucceeded, 'consistency check failed').to.be.true;
        expect(reorgDetectorSucceeded, 'reorg detection check failed').to.be.true;
    });

    step('check basic EN Web3 endpoints', async () => {
        const blockNumber = await externalNode.getBlockNumber();
        console.log(`Block number on EN: ${blockNumber}`);
        expect(blockNumber).to.be.greaterThan(snapshotMetadata.miniblockNumber);
        const l1BatchNumber = await externalNode.getL1BatchNumber();
        console.log(`L1 batch number on EN: ${l1BatchNumber}`);
        expect(l1BatchNumber).to.be.greaterThan(snapshotMetadata.l1BatchNumber);
    });

    step('check tokens on EN', async () => {
        const externalNodeTokens = await getAllTokens(externalNode);
        console.log('Fetched tokens from EN', externalNodeTokens);
        expect(externalNodeTokens).to.not.be.empty; // should contain at least ether
        const mainNodeTokens = await getAllTokens(mainNode, snapshotMetadata.miniblockNumber);
        console.log('Fetched tokens from main node', mainNodeTokens);

        // Sort tokens deterministically, since the order in which they are returned is not guaranteed.
        const compareFn = (x: TokenInfo, y: TokenInfo) => {
            expect(x.l2_address, 'Multiple tokens with same L2 address').to.not.equal(y.l2_address);
            return x.l2_address < y.l2_address ? -1 : 1;
        };
        externalNodeTokens.sort(compareFn);
        mainNodeTokens.sort(compareFn);

        expect(mainNodeTokens).to.deep.equal(externalNodeTokens);
    });

    step('check EVM contract', async () => {
        if (erc20Address === undefined) {
            console.log('EVM contracts are disabled; skipping');
            return;
        }
        console.log('Checking EVM contract', erc20Address);
        const erc20 = new ethers.Contract(erc20Address, erc20Abi, externalNode);
        const symbol = await erc20.getFunction('symbol').staticCall();
        expect(symbol).to.equal('TEST');
    });

    step('restart EN', async () => {
        console.log('Stopping external node');
        await externalNodeProcess.stopAndWait();

        const components = disableTreeDuringPruning
            ? NodeComponents.WITH_TREE_FETCHER_AND_NO_TREE
            : NodeComponents.WITH_TREE_FETCHER;
        const pruningParams = {
            EN_PRUNING_ENABLED: 'true',
            EN_PRUNING_REMOVAL_DELAY_SEC: '1',
            EN_PRUNING_DATA_RETENTION_SEC: '0', // immediately prune executed batches
            EN_PRUNING_CHUNK_SIZE: '1'
        };
        externalNodeEnv = { ...externalNodeEnv, ...pruningParams };

        if (fileConfig.loadFromFile) {
            setChunkSize(pathToHome, fileConfig, 1);
            setDataRetentionSec(pathToHome, fileConfig, 0);
            setRemovalDelaySec(pathToHome, fileConfig, 1);
        }

        console.log('Starting EN with pruning params', pruningParams);
        externalNodeProcess = await NodeProcess.spawn(
            externalNodeEnv,
            externalNodeProcess.logs,
            pathToHome,
            components,
            fileConfig.loadFromFile,
            fileConfig.chain
        );

        let isDbPrunerReady = false;
        let isTreePrunerReady = disableTreeDuringPruning; // skip health checks if we don't run the tree
        let isTreeFetcherReady = false;
        while (!isDbPrunerReady || !isTreePrunerReady || !isTreeFetcherReady) {
            await sleep(1000);
            const health = await getExternalNodeHealth(extNodeHealthUrl);
            if (health === null) {
                continue;
            }

            if (!isDbPrunerReady) {
                console.log('DB pruner health', health.components.db_pruner);
                const status = health.components.db_pruner?.status;
                expect(status).to.be.oneOf([undefined, 'not_ready', 'affected', 'ready']);
                isDbPrunerReady = status === 'ready';
            }
            if (!isTreePrunerReady) {
                console.log('Tree pruner health', health.components.tree_pruner);
                const status = health.components.tree_pruner?.status;
                expect(status).to.be.oneOf([undefined, 'not_ready', 'affected', 'ready']);
                isTreePrunerReady = status === 'ready';
            }
            if (!isTreeFetcherReady) {
                console.log('Tree fetcher health', health.components.tree_data_fetcher);
                const status = health.components.tree_data_fetcher?.status;
                expect(status).to.be.oneOf([undefined, 'not_ready', 'affected', 'ready']);
                isTreeFetcherReady = status === 'ready';
            }
        }
    });

    // The logic below works fine if there is other transaction activity on the test network; we still
    // create *at least* `PRUNED_BATCH_COUNT + 1` L1 batches; thus, at least `PRUNED_BATCH_COUNT` of them
    // should be pruned eventually.
    step(`generate ${PRUNED_BATCH_COUNT + 1} L1 batches`, async () => {
        for (let i = 0; i < PRUNED_BATCH_COUNT + 1; i++) {
            await fundedWallet.generateL1Batch();
        }
    });

    step(`wait for pruning ${PRUNED_BATCH_COUNT} L1 batches`, async () => {
        const expectedPrunedBatchNumber = snapshotMetadata.l1BatchNumber + PRUNED_BATCH_COUNT;
        console.log(`Waiting for L1 batch #${expectedPrunedBatchNumber} to be pruned`);
        let isDbPruned = false;
        let isTreePruned = disableTreeDuringPruning;

        while (!isDbPruned || !isTreePruned) {
            await sleep(1000);
            const health = (await getExternalNodeHealth(extNodeHealthUrl))!;

            const dbPrunerHealth = health.components.db_pruner!;
            console.log('DB pruner health', dbPrunerHealth);
            expect(dbPrunerHealth.status).to.be.equal('ready');
            isDbPruned = dbPrunerHealth.details!.last_hard_pruned_l1_batch! >= expectedPrunedBatchNumber;

            if (disableTreeDuringPruning) {
                expect(health.components.tree).to.be.undefined;
            } else {
                const treeHealth = health.components.tree!;
                console.log('Tree health', treeHealth);
                expect(treeHealth.status).to.be.equal('ready');
                const minTreeL1BatchNumber = treeHealth.details?.min_l1_batch_number;
                // The batch number pruned from the tree is one less than `minTreeL1BatchNumber`.
                isTreePruned = minTreeL1BatchNumber ? minTreeL1BatchNumber - 1 >= expectedPrunedBatchNumber : false;
            }
        }
    });
});

async function decompressGzip(filePath: string): Promise<Buffer> {
    const readStream = (await fs.open(filePath)).createReadStream();
    return new Promise((resolve, reject) => {
        const gunzip = zlib.createGunzip();
        let chunks: Uint8Array[] = [];

        gunzip.on('data', (chunk) => chunks.push(chunk));
        gunzip.on('end', () => resolve(Buffer.concat(chunks)));
        gunzip.on('error', reject);
        readStream.pipe(gunzip);
    });
}

async function createSnapshot(useZkStack: boolean) {
    let command = '';
    if (useZkStack) {
        command = `zkstack dev snapshot create`;
        command += ` --chain ${fileConfig.chain}`;
    } else {
        command = `zk run snapshots-creator`;
    }
    await executeCommandWithLogs(command, 'snapshot-creator.log');
}
