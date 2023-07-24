import { setImmediate as setImmediatePromise } from 'timers/promises'
import BN from 'bn.js'
import type { PeerId } from '@libp2p/interface-peer-id'
import { peerIdFromString } from '@libp2p/peer-id'
import chalk from 'chalk'
import { EventEmitter } from 'events'
import { Multiaddr } from '@multiformats/multiaddr'
import {
  defer,
  stringToU8a,
  ChannelStatus,
  Address,
  ChannelEntry,
  AccountEntry,
  PublicKey,
  Snapshot,
  debug,
  retryWithBackoffThenThrow,
  Balance,
  BalanceType,
  ordered,
  u8aToHex,
  FIFO,
  type DeferType,
  type Ticket,
  create_counter,
  create_multi_counter,
  create_gauge,
  create_multi_gauge,
  U256,
  random_integer,
  Hash,
  number_to_channel_status
} from '@hoprnet/hopr-utils'

import type { ChainWrapper } from '../ethereum.js'
import {
  type Event,
  type EventNames,
  type IndexerEvents,
  type TokenEvent,
  type TokenEventNames,
  type RegistryEvent,
  type RegistryEventNames,
  type IndexerEventEmitter,
  IndexerStatus
} from './types.js'
import { isConfirmedBlock, snapshotComparator, type IndexerSnapshot } from './utils.js'
import { BigNumber, type Contract, errors } from 'ethers'

import {
  CORE_ETHEREUM_CONSTANTS,
  Ethereum_AccountEntry,
  Ethereum_Address,
  Ethereum_Balance,
  Ethereum_ChannelEntry,
  Ethereum_Database,
  Ethereum_Hash,
  Ethereum_PublicKey,
  Ethereum_Snapshot
} from '../db.js'

import type { TypedEvent, TypedEventFilter } from '../utils/common.js'

// @ts-ignore untyped library
import retimer from 'retimer'
import assert from 'assert'

// Exported from Rust
const constants = CORE_ETHEREUM_CONSTANTS()

const log = debug('hopr-core-ethereum:indexer')
const verbose = debug('hopr-core-ethereum:verbose:indexer')

const getSyncPercentage = (start: number, current: number, end: number) =>
  (((current - start) / (end - start)) * 100).toFixed(2)
const backoffOption: Parameters<typeof retryWithBackoffThenThrow>[1] = { maxDelay: constants.MAX_TRANSACTION_BACKOFF }

// Metrics
const metric_indexerErrors = create_multi_counter(
  'core_ethereum_mcounter_indexer_provider_errors',
  'Multicounter for provider errors in Indexer',
  ['type']
)
const metric_unconfirmedBlocks = create_counter(
  'core_ethereum_counter_indexer_processed_unconfirmed_blocks',
  'Number of processed unconfirmed blocks'
)
const metric_numAnnouncements = create_counter(
  'core_ethereum_counter_indexer_announcements',
  'Number of processed announcements'
)
const metric_blockNumber = create_gauge('core_ethereum_gauge_indexer_block_number', 'Current block number')
const metric_channelStatus = create_multi_gauge(
  'core_ethereum_gauge_indexer_channel_status',
  'Status of different channels',
  ['channel']
)
const metric_ticketsRedeemed = create_counter(
  'core_ethereum_counter_indexer_tickets_redeemed',
  'Number of redeemed tickets'
)

/**
 * Indexes HoprChannels smart contract and stores to the DB,
 * all channels in the network.
 * Also keeps track of the latest block number.
 */
class Indexer extends (EventEmitter as new () => IndexerEventEmitter) {
  public status: IndexerStatus = IndexerStatus.STOPPED
  public latestBlock: number = 0 // latest known on-chain block number
  public startupBlock: number = 0 // blocknumber at which the indexer starts

  // Use FIFO + sliding window for many events
  private unconfirmedEvents: FIFO<TypedEvent<any, any>>

  private chain: ChainWrapper
  private genesisBlock: number
  private lastSnapshot: IndexerSnapshot | undefined

  private blockProcessingLock: DeferType<void> | undefined

  private unsubscribeErrors: () => void
  private unsubscribeBlock: () => void

  constructor(
    private address: Address,
    private db: Ethereum_Database,
    private maxConfirmations: number,
    private blockRange: number
  ) {
    super()

    this.unconfirmedEvents = FIFO<TypedEvent<any, any>>()
  }

  /**
   * Starts indexing.
   */
  public async start(chain: ChainWrapper, genesisBlock: number): Promise<void> {
    if (this.status === IndexerStatus.STARTED) {
      return
    }
    this.status = IndexerStatus.STARTING

    log(`Starting indexer...`)
    this.chain = chain
    this.genesisBlock = genesisBlock

    const [latestSavedBlock, latestOnChainBlock] = await Promise.all([
      new BN(await this.db.get_latest_block_number()).toNumber(),
      this.chain.getLatestBlockNumber()
    ])

    this.latestBlock = latestOnChainBlock
    this.startupBlock = latestOnChainBlock

    log('Latest saved block %d', latestSavedBlock)
    log('Latest on-chain block %d', latestOnChainBlock)

    // go back 'MAX_CONFIRMATIONS' blocks in case of a re-org at time of stopping
    let fromBlock = latestSavedBlock
    if (fromBlock - this.maxConfirmations > 0) {
      fromBlock = fromBlock - this.maxConfirmations
    }
    // no need to query before HoprChannels or HoprNetworkRegistry existed
    fromBlock = Math.max(fromBlock, this.genesisBlock)

    log('Starting to index from block %d, sync progress 0%%', fromBlock)

    const orderedBlocks = ordered<number>()

    // Starts the asynchronous stream of indexer events
    // and feeds them to the event listener
    ;(async function (this: Indexer) {
      for await (const block of orderedBlocks.iterator()) {
        await this.onNewBlock(block.value, true, true) // exceptions are handled (for real)
      }
    }).call(this)

    // Do not process new blocks before querying old blocks has finished
    const newBlocks = ordered<number>()

    const unsubscribeBlock = this.chain.subscribeBlock(async (block: number) => {
      newBlocks.push({
        index: block,
        value: block
      })
    })

    this.unsubscribeBlock = () => {
      unsubscribeBlock()
      newBlocks.end()
      orderedBlocks.end()
    }

    this.unsubscribeErrors = this.chain.subscribeError(async (error: any) => {
      await this.onProviderError(error) // exceptions are handled
    })

    // get past events
    fromBlock = await this.processPastEvents(fromBlock, latestOnChainBlock, this.blockRange)

    // Feed new blocks to the ordered queue
    ;(async function (this: Indexer) {
      for await (const newBlock of newBlocks.iterator()) {
        orderedBlocks.push(newBlock) // exceptions are handled
      }
    }).call(this)

    log('Subscribing to events from block %d', fromBlock)

    this.status = IndexerStatus.STARTED
    this.emit('status', IndexerStatus.STARTED)
    log(chalk.green('Indexer started!'))
  }

  /**
   * Stops indexing.
   */
  public async stop(): Promise<void> {
    if (this.status === IndexerStatus.STOPPED) {
      return
    }

    log(`Stopping indexer...`)

    this.unsubscribeBlock()
    this.unsubscribeErrors()

    this.blockProcessingLock && (await this.blockProcessingLock.promise)

    this.status = IndexerStatus.STOPPED
    this.emit('status', IndexerStatus.STOPPED)
    log(chalk.green('Indexer stopped!'))
  }

  /**
   * Restarts the indexer
   * @returns a promise that resolves once the indexer
   * has been restarted
   */
  protected async restart(): Promise<void> {
    if (this.status === 'restarting') {
      return
    }

    log('Indexer restaring')

    try {
      this.status = IndexerStatus.RESTARTING

      this.stop()
      await this.start(this.chain, this.genesisBlock)
    } catch (err) {
      this.status = IndexerStatus.STOPPED
      this.emit('status', IndexerStatus.STOPPED)
      log(chalk.red('Failed to restart: %s', err.message))
      throw err
    }
  }

  /**
   * Gets all interesting on-chain events, such as Transfer events and payment
   * channel events
   * @param fromBlock block to start from
   * @param toBlock last block (inclusive) to consider
   * towards or from the node towards someone else
   * @returns all relevant events in the specified block range
   */
  private async getEvents(
    fromBlock: number,
    toBlock: number,
    fetchTokenTransactions = false
  ): Promise<
    | {
        success: true
        events: TypedEvent<any, any>[]
      }
    | {
        success: false
      }
  > {
    let rawEvents: TypedEvent<any, any>[] = []

    const queries: { contract: Contract; filter: TypedEventFilter<any> }[] = [
      // HoprChannels
      {
        contract: this.chain.getChannels() as unknown as Contract,
        filter: {
          topics: [
            [
              // Relevant channel events
              this.chain.getChannels().interface.getEventTopic('Announcement'),
              this.chain.getChannels().interface.getEventTopic('ChannelUpdated'),
              this.chain.getChannels().interface.getEventTopic('TicketRedeemed')
            ]
          ]
        }
      },
      // HoprNetworkRegistry
      {
        contract: this.chain.getNetworkRegistry() as unknown as Contract,
        filter: {
          topics: [
            [
              // Relevant HoprNetworkRegistry events
              this.chain.getNetworkRegistry().interface.getEventTopic('Registered'),
              this.chain.getNetworkRegistry().interface.getEventTopic('Deregistered'),
              this.chain.getNetworkRegistry().interface.getEventTopic('RegisteredByOwner'),
              this.chain.getNetworkRegistry().interface.getEventTopic('DeregisteredByOwner'),
              this.chain.getNetworkRegistry().interface.getEventTopic('EligibilityUpdated'),
              this.chain.getNetworkRegistry().interface.getEventTopic('EnabledNetworkRegistry')
            ]
          ]
        }
      }
    ]

    // Token events
    // Actively query for logs to prevent polling done by Ethers.js
    // that don't retry on failed attempts and thus makes the indexer
    // handle errors produced by internal Ethers.js provider calls
    if (fetchTokenTransactions) {
      queries.push({
        contract: this.chain.getToken() as unknown as Contract,
        filter: {
          topics: [
            // Token transfer *from* us
            [this.chain.getToken().interface.getEventTopic('Transfer')],
            [u8aToHex(this.address.to_bytes32())]
          ]
        }
      })
      queries.push({
        contract: this.chain.getToken() as unknown as Contract,
        filter: {
          topics: [
            // Token transfer *towards* us
            [this.chain.getToken().interface.getEventTopic('Transfer')],
            null,
            [u8aToHex(this.address.to_bytes32())]
          ]
        }
      })
    }

    for (const query of queries) {
      let tmpEvents: TypedEvent<any, any>[]
      try {
        tmpEvents = (await query.contract.queryFilter(query.filter, fromBlock, toBlock)) as any
      } catch {
        return {
          success: false
        }
      }

      for (const event of tmpEvents) {
        Object.assign(event, query.contract.interface.parseLog(event))

        if (event.event == undefined) {
          Object.assign(event, { event: (event as any).name })
        }
        rawEvents.push(event)
      }
    }

    // sort in-place
    rawEvents.sort(snapshotComparator)

    return {
      success: true,
      events: rawEvents
    }
  }

  /**
   * Query past events, this will loop until it gets all blocks from `toBlock` to `fromBlock`.
   * If we exceed response pull limit, we switch into quering smaller chunks.
   * TODO: optimize DB and fetch requests
   * @param fromBlock
   * @param maxToBlock
   * @param maxBlockRange
   * @return past events and last queried block
   */
  private async processPastEvents(fromBlock: number, maxToBlock: number, maxBlockRange: number): Promise<number> {
    const start = fromBlock
    let failedCount = 0

    while (fromBlock < maxToBlock) {
      const blockRange = failedCount > 0 ? Math.floor(maxBlockRange / 4 ** failedCount) : maxBlockRange
      // should never be above maxToBlock
      let toBlock = Math.min(fromBlock + blockRange, maxToBlock)

      // log(
      //   `${failedCount > 0 ? 'Re-quering' : 'Quering'} past events from %d to %d: %d`,
      //   fromBlock,
      //   toBlock,
      //   toBlock - fromBlock
      // )

      let res = await this.getEvents(fromBlock, toBlock)

      if (res.success) {
        this.onNewEvents(res.events)
        await this.onNewBlock(toBlock, false, false, true)
      } else {
        failedCount++

        if (failedCount > 5) {
          throw Error(`Could not fetch logs from block ${fromBlock} to ${toBlock}. Giving up`)
        }

        await setImmediatePromise()

        continue
      }

      failedCount = 0
      fromBlock = toBlock

      log('Sync progress %d% @ block %d', getSyncPercentage(start, fromBlock, maxToBlock), toBlock)
    }

    return fromBlock
  }

  /**
   * Called whenever there was a provider error.
   * Will restart the indexer if needed.
   * @param error
   * @private
   */
  private async onProviderError(error: any): Promise<void> {
    if (String(error).match(/eth_blockNumber/)) {
      verbose(`Ignoring failed "eth_blockNumber" provider call from Ethers.js`)
      return
    }

    log(chalk.red(`etherjs error: ${error}`))

    try {
      const errorType = [errors.SERVER_ERROR, errors.TIMEOUT, 'ECONNRESET', 'ECONNREFUSED'].filter((err) =>
        [error?.code, String(error)].includes(err)
      )

      // if provider connection issue
      if (errorType.length != 0) {
        metric_indexerErrors.increment([errorType[0]])

        log(chalk.blue('code error falls here', this.chain.getAllQueuingTransactionRequests().length))
        // allow the indexer to restart even there is no transaction in queue
        await retryWithBackoffThenThrow(
          () =>
            Promise.allSettled([
              ...this.chain.getAllQueuingTransactionRequests().map((request) => {
                // convert TransactionRequest to signed transaction and send out
                return this.chain.sendTransaction(
                  true,
                  { to: request.to, value: request.value as BigNumber, data: request.data as string },
                  // the transaction is resolved without the specific tag for its action, but rather as a result of provider retry
                  (txHash: string) => this.resolvePendingTransaction(`on-provider-error-${txHash}`, txHash)
                )
              }),
              this.restart()
            ]),
          backoffOption
        )
      } else {
        metric_indexerErrors.increment(['unknown'])
        await retryWithBackoffThenThrow(() => this.restart(), backoffOption)
      }
    } catch (err) {
      log(`error: exception while processing another provider error ${error}`, err)
    }
  }

  /**
   * Called whenever a new block found.
   * This will update `this.latestBlock`,
   * and processes events which are within
   * confirmed blocks.
   * @param blockNumber latest on-chain block number
   * @param fetchEvents [optional] if true, query provider for events in block
   */
  private async onNewBlock(
    blockNumber: number,
    fetchEvents = false,
    fetchNativeTxs = false,
    blocking = false
  ): Promise<void> {
    // NOTE: This function is also used in event handlers
    // where it cannot be 'awaited', so all exceptions need to be caught.

    // Don't process any block if indexer was stopped.
    if (![IndexerStatus.STARTING, IndexerStatus.STARTED].includes(this.status)) {
      return
    }

    // Set a lock during block processing to make sure database does not get closed
    if (this.blockProcessingLock) {
      this.blockProcessingLock.resolve()
    }
    this.blockProcessingLock = defer<void>()

    const currentBlock = blockNumber - this.maxConfirmations

    if (currentBlock < 0) {
      return
    }

    log('Indexer got new block %d, handling block %d', blockNumber, blockNumber - this.maxConfirmations)
    this.emit('block', blockNumber)

    // update latest block
    this.latestBlock = Math.max(this.latestBlock, blockNumber)
    metric_blockNumber.set(this.latestBlock)

    let fetchedSnapshot = await this.db.get_latest_confirmed_snapshot()
    let lastDatabaseSnapshot =
      fetchedSnapshot === undefined ? fetchedSnapshot : Snapshot.deserialize(fetchedSnapshot.serialize())

    // settle transactions before processing events
    if (fetchNativeTxs) {
      // get the number of unconfirmed (pending and mined) transactions tracked by the transaction manager
      const unconfirmedTxListeners = this.chain.getAllUnconfirmedHash()
      // only request transactions in block when transaction manager is tracking
      log('Indexer fetches native txs for %d unconfirmed tx listeners', unconfirmedTxListeners.length)
      if (unconfirmedTxListeners.length > 0) {
        let nativeTxHashes: string[] | undefined
        try {
          // This new block marks a previous block
          // (blockNumber - this.maxConfirmations) is final.
          // Confirm native token transactions in that previous block.
          nativeTxHashes = await this.chain.getTransactionsInBlock(currentBlock)
        } catch (err) {
          log(
            `error: failed to retrieve information about block ${currentBlock} with finality ${this.maxConfirmations}`,
            err
          )
        }

        // update transaction manager after updating db
        if (nativeTxHashes && nativeTxHashes.length > 0) {
          // @TODO replace this with some efficient set intersection algorithm
          for (const txHash of nativeTxHashes) {
            if (this.listeners(`withdraw-native-${txHash}`).length > 0) {
              this.indexEvent(`withdraw-native-${txHash}`)
            } else if (this.listeners(`withdraw-hopr-${txHash}`).length > 0) {
              this.indexEvent(`withdraw-hopr-${txHash}`)
            } else if (this.listeners(`announce-${txHash}`).length > 0) {
              this.indexEvent(`announce-${txHash}`)
            } else if (this.listeners(`channel-updated-${txHash}`).length > 0) {
              this.indexEvent(`channel-updated-${txHash}`)
            }

            // update transaction manager
            this.chain.updateConfirmedTransaction(txHash)
          }
        }
      }
    }

    if (fetchEvents) {
      // Don't fail immediately when one block is temporarily not available
      const RETRIES = 3
      let res: Awaited<ReturnType<Indexer['getEvents']>>

      for (let i = 0; i < RETRIES; i++) {
        log(
          `fetchEvents at currentBlock ${currentBlock} startupBlock ${this.startupBlock} maxConfirmations ${
            this.maxConfirmations
          }. ${currentBlock > this.startupBlock + this.maxConfirmations}`
        )
        if (currentBlock > this.startupBlock) {
          // between starting block "Latest on-chain block" and finality + 1 to prevent double processing of events in blocks ["Latest on-chain block" - maxConfirmations, "Latest on-chain block"]
          res = await this.getEvents(currentBlock, currentBlock, true)
        } else {
          res = await this.getEvents(currentBlock, currentBlock, false)
        }

        if (res.success) {
          this.onNewEvents(res.events)
          break
        } else if (i + 1 < RETRIES) {
          await setImmediatePromise()
        } else {
          log(`Cannot fetch block ${currentBlock} despite ${RETRIES} retries. Skipping block.`)
        }
      }
    }

    try {
      await this.processUnconfirmedEvents(blockNumber, lastDatabaseSnapshot, blocking)
    } catch (err) {
      log(`error while processing unconfirmed events`, err)
    }

    // resend queuing transactions, when there are transactions (in queue) that haven't been accepted by the RPC
    // and resend transactions if the current balance is sufficient.

    const allQueuingTxs = this.chain.getAllQueuingTransactionRequests()
    if (allQueuingTxs.length > 0) {
      const minimumBalanceForQueuingTxs = allQueuingTxs.reduce(
        (acc, queuingTx) =>
          // to get the minimum balance required to resend a queuing transaction,
          // use the gasLimit (that shouldn't change, unless the contract state is different)
          // multiplies the maxFeePerGas of the queuing transaction
          acc.add(new BN(queuingTx.gasLimit.toString()).mul(new BN(queuingTx.maxFeePerGas.toString()))),
        new BN(0)
      )
      const currentBalance = await this.chain.getNativeBalance(this.address)
      if (
        // compare the current balance with the minimum balance required at the time of transaction being queued.
        // NB: Both gasLimit and maxFeePerGas requirement may be different due to "drastic" changes in contract state and network condition
        new BN(currentBalance.amount().to_string()).gte(minimumBalanceForQueuingTxs)
      ) {
        try {
          await Promise.all(
            allQueuingTxs.map((request) => {
              // convert TransactionRequest to signed transaction and send out
              return this.chain.sendTransaction(
                true,
                { to: request.to, value: request.value as BigNumber, data: request.data as string },
                (txHash: string) => this.resolvePendingTransaction(`on-new-block-${txHash}`, txHash)
              )
            })
          )
        } catch (err) {
          log(`error: failed to send queuing transaction on new block`, err)
        }
      }
    }

    try {
      await this.db.update_latest_block_number(blockNumber)
    } catch (err) {
      log(`error: failed to update database with latest block number ${blockNumber}`, err)
    }

    this.blockProcessingLock.resolve()

    this.emit('block-processed', currentBlock)
  }

  /**
   * Adds new events to the queue of unprocessed events
   * @dev ignores events that have been processed before.
   * @param events new unprocessed events
   */
  private onNewEvents(events: Event<any>[] | TokenEvent<any>[] | RegistryEvent<any>[] | undefined): void {
    if (events == undefined || events.length == 0) {
      // Nothing to do
      return
    }

    let offset = 0

    // lastSnapshot ~= watermark of previously process events
    //
    // lastSnapshot == undefined means there is no watermark, hence
    // all events can be considered new
    if (this.lastSnapshot != undefined) {
      let currentSnapshot: IndexerSnapshot = {
        blockNumber: events[offset].blockNumber,
        logIndex: events[offset].logIndex,
        transactionIndex: events[offset].transactionIndex
      }

      // As long events are older than the current watermark,
      // increase the offset to ignore them
      while (snapshotComparator(this.lastSnapshot, currentSnapshot) >= 0) {
        offset++
        if (offset < events.length) {
          currentSnapshot = {
            blockNumber: events[offset].blockNumber,
            logIndex: events[offset].logIndex,
            transactionIndex: events[offset].transactionIndex
          }
        } else {
          break
        }
      }
    }

    // Once the offset is known upon which we have
    // received new events, add them to `unconfirmedEvents` to
    // be processed once the next+confirmationTime block
    // has been mined
    for (; offset < events.length; offset++) {
      this.unconfirmedEvents.push(events[offset])
    }

    // Update watermark for next iteration
    this.lastSnapshot = {
      blockNumber: events[events.length - 1].blockNumber,
      logIndex: events[events.length - 1].logIndex,
      transactionIndex: events[events.length - 1].transactionIndex
    }
  }

  /**
   * Process all stored but not yet processed events up to latest
   * confirmed block (latestBlock - confirmationTime)
   * @param blockNumber latest on-chain block number
   * @param lastDatabaseSnapshot latest snapshot in database
   */
  async processUnconfirmedEvents(blockNumber: number, lastDatabaseSnapshot: Snapshot | undefined, blocking: boolean) {
    log(
      'At the new block %d, there are %i unconfirmed events and ready to process %s, because the event was mined at %i (with finality %i)',
      blockNumber,
      this.unconfirmedEvents.size(),
      this.unconfirmedEvents.size() > 0
        ? isConfirmedBlock(this.unconfirmedEvents.peek().blockNumber, blockNumber, this.maxConfirmations)
        : null,
      this.unconfirmedEvents.size() > 0 ? this.unconfirmedEvents.peek().blockNumber : 0,
      this.maxConfirmations
    )

    // check unconfirmed events and process them if found
    // to be within a confirmed block
    while (
      this.unconfirmedEvents.size() > 0 &&
      isConfirmedBlock(this.unconfirmedEvents.peek().blockNumber, blockNumber, this.maxConfirmations)
    ) {
      const event = this.unconfirmedEvents.shift()
      log(
        'Processing event %s blockNumber=%s maxConfirmations=%s',
        // @TODO: fix type clash
        event.event,
        blockNumber,
        this.maxConfirmations
      )

      // if we find a previous snapshot, compare event's snapshot with last processed
      if (lastDatabaseSnapshot) {
        const lastSnapshotComparison = snapshotComparator(event, {
          blockNumber: lastDatabaseSnapshot.block_number.as_u32(),
          logIndex: lastDatabaseSnapshot.log_index.as_u32(),
          transactionIndex: lastDatabaseSnapshot.transaction_index.as_u32()
        })

        // check if this is a duplicate or older than last snapshot
        // ideally we would have detected if this snapshot was indeed processed,
        // at the moment we don't keep all events stored as we intend to keep
        // this indexer very simple
        if (lastSnapshotComparison == 0 || lastSnapshotComparison < 0) {
          continue
        }
      }

      // @TODO: fix type clash
      const eventName = event.event as EventNames | TokenEventNames | RegistryEventNames

      lastDatabaseSnapshot = new Snapshot(
        new U256(event.blockNumber.toString()),
        new U256(event.transactionIndex.toString()),
        new U256(event.logIndex.toString())
      )

      log('Event name %s and hash %s', eventName, event.transactionHash)

      switch (eventName) {
        case 'Announcement':
        case 'Announcement(address,bytes,bytes)':
          await this.onAnnouncement(
            event as Event<'Announcement'>,
            new BN(blockNumber.toPrecision()),
            lastDatabaseSnapshot
          )
          break
        case 'ChannelUpdated':
        case 'ChannelUpdated(address,address,tuple)':
          await this.onChannelUpdated(event as Event<'ChannelUpdated'>, lastDatabaseSnapshot)
          break
        case 'Transfer':
        case 'Transfer(address,address,uint256)':
          // handle HOPR token transfer
          await this.onTransfer(event as TokenEvent<'Transfer'>, lastDatabaseSnapshot)
          break
        case 'TicketRedeemed':
        case 'TicketRedeemed(address,address,bytes32,uint256,uint256,bytes32,uint256,uint256,bytes)':
          // if unlock `outstandingTicketBalance`, if applicable
          await this.onTicketRedeemed(event as Event<'TicketRedeemed'>, lastDatabaseSnapshot)
          break
        case 'EligibilityUpdated':
        case 'EligibilityUpdated(address,bool)':
          await this.onEligibilityUpdated(event as RegistryEvent<'EligibilityUpdated'>, lastDatabaseSnapshot)
          break
        case 'Registered':
        case 'Registered(address,string)':
        case 'RegisteredByOwner':
        case 'RegisteredByOwner(address,string)':
          await this.onRegistered(
            event as RegistryEvent<'Registered'> | RegistryEvent<'RegisteredByOwner'>,
            lastDatabaseSnapshot
          )
          break
        case 'Deregistered':
        case 'Deregistered(address,string)':
        case 'DeregisteredByOwner':
        case 'DeregisteredByOwner(address,string)':
          await this.onDeregistered(
            event as RegistryEvent<'Deregistered'> | RegistryEvent<'DeregisteredByOwner'>,
            lastDatabaseSnapshot
          )
          break
        case 'EnabledNetworkRegistry':
        case 'EnabledNetworkRegistry(bool)':
          await this.onEnabledNetworkRegistry(event as RegistryEvent<'EnabledNetworkRegistry'>, lastDatabaseSnapshot)
          break
        default:
          log(`ignoring event '${String(eventName)}'`)
      }

      metric_unconfirmedBlocks.increment()

      if (
        !blocking &&
        this.unconfirmedEvents.size() > 0 &&
        isConfirmedBlock(this.unconfirmedEvents.peek().blockNumber, blockNumber, this.maxConfirmations)
      ) {
        // Give other tasks CPU time to happen
        // Wait until end of next event loop iteration before starting next db write-back
        await setImmediatePromise()
      }
    }
  }

  private async onAnnouncement(event: Event<'Announcement'>, blockNumber: BN, lastSnapshot: Snapshot): Promise<void> {
    // publicKey given by the SC is verified
    const publicKey = PublicKey.deserialize(stringToU8a(event.args.publicKey))

    let multiaddr: Multiaddr
    try {
      multiaddr = new Multiaddr(stringToU8a(event.args.multiaddr))
        // remove "p2p" and corresponding peerID
        .decapsulateCode(421)
        // add new peerID
        .encapsulate(`/p2p/${publicKey.to_peerid_str()}`)
    } catch (error) {
      log(`Invalid multiaddr '${event.args.multiaddr}' given in event 'onAnnouncement'`)
      log(error)
      return
    }

    const account = new AccountEntry(publicKey, multiaddr.toString(), blockNumber.toNumber())

    log('New node announced', account.get_address().to_hex(), account.get_multiaddress_str())
    metric_numAnnouncements.increment()

    assert(lastSnapshot !== undefined)
    await this.db.update_account_and_snapshot(
      Ethereum_AccountEntry.deserialize(account.serialize()),
      Ethereum_Snapshot.deserialize(lastSnapshot.serialize())
    )

    this.emit('peer', {
      id: peerIdFromString(account.get_peer_id_str()),
      multiaddrs: [new Multiaddr(account.get_multiaddress_str())]
    })
  }

  private async onChannelUpdated(event: Event<'ChannelUpdated'>, lastSnapshot: Snapshot): Promise<void> {
    const { source, destination, newState } = event.args

    log('channel-updated for hash %s', event.transactionHash)
    let channel = new ChannelEntry(
      Address.from_string(source),
      Address.from_string(destination),
      new Balance(newState.balance.toString(), BalanceType.HOPR),
      new Hash(stringToU8a(newState.commitment)),
      new U256(newState.ticketEpoch.toString()),
      new U256(newState.ticketIndex.toString()),
      number_to_channel_status(newState.status),
      new U256(newState.channelEpoch.toString()),
      new U256(newState.closureTime.toString())
    )
    log(channel.to_string())

    let prevState: ChannelEntry
    let channel_entry = await this.db.get_channel(Ethereum_Hash.deserialize(channel.get_id().serialize()))
    if (channel_entry !== undefined) {
      prevState = ChannelEntry.deserialize(channel_entry.serialize())
    }

    assert(lastSnapshot !== undefined)
    await this.db.update_channel_and_snapshot(
      Ethereum_Hash.deserialize(channel.get_id().serialize()),
      Ethereum_ChannelEntry.deserialize(channel.serialize()),
      Ethereum_Snapshot.deserialize(lastSnapshot.serialize())
    )

    metric_channelStatus.set([channel.get_id().to_hex()], channel.status)

    if (prevState && channel.status == ChannelStatus.Closed && prevState.status != ChannelStatus.Closed) {
      log('channel was closed')
      await this.onChannelClosed(channel)
    }

    this.emit('channel-update', channel)
    verbose(`channel-update for channel ${channel.get_id().to_hex()}`)

    if (channel.source.eq(this.address) || channel.destination.eq(this.address)) {
      this.emit('own-channel-updated', channel)

      if (channel.destination.eq(this.address)) {
        // Channel _to_ us
        if (channel.status === ChannelStatus.WaitingForCommitment) {
          log('channel to us waiting for commitment')
          log(channel.to_string())
          this.emit('channel-waiting-for-commitment', channel)
        }
      }
    }
  }

  private async onTicketRedeemed(event: Event<'TicketRedeemed'>, lastSnapshot: Snapshot) {
    if (Address.from_string(event.args.source).eq(this.address)) {
      // the node used to lock outstandingTicketBalance
      // rebuild part of the Ticket
      const partialTicket: Partial<Ticket> = {
        counterparty: Address.from_string(event.args.destination),
        amount: new Balance(event.args.amount.toString(), BalanceType.HOPR)
      }
      const outstandingBalance = Balance.deserialize(
        (
          await this.db.get_pending_balance_to(Ethereum_Address.deserialize(partialTicket.counterparty.serialize()))
        ).serialize_value(),
        BalanceType.HOPR
      )

      assert(lastSnapshot !== undefined)
      try {
        // Negative case:
        // It falls into this case when db of sender gets erased while having tickets pending.
        // TODO: handle this may allow sender to send arbitrary amount of tickets through open
        // channels with positive balance, before the counterparty initiates closure.
        const balance = outstandingBalance.lte(Balance.zero(BalanceType.HOPR))
          ? Balance.zero(BalanceType.HOPR)
          : outstandingBalance
        await this.db.resolve_pending(
          Ethereum_Address.deserialize(partialTicket.counterparty.serialize()),
          Ethereum_Balance.deserialize(balance.serialize_value(), BalanceType.HOPR),
          Ethereum_Snapshot.deserialize(lastSnapshot.serialize())
        )
        metric_ticketsRedeemed.increment()
      } catch (error) {
        log(`error in onTicketRedeemed ${error}`)
        throw new Error(`error in onTicketRedeemed ${error}`)
      }
    }
  }

  private async onChannelClosed(channel: ChannelEntry) {
    await this.db.delete_acknowledged_tickets_from(Ethereum_ChannelEntry.deserialize(channel.serialize()))
    this.emit('channel-closed', channel)
  }

  private async onEligibilityUpdated(
    event: RegistryEvent<'EligibilityUpdated'>,
    lastSnapshot: Snapshot
  ): Promise<void> {
    assert(lastSnapshot !== undefined)

    const account = Address.from_string(event.args.account)
    await this.db.set_eligible(
      Ethereum_Address.deserialize(account.serialize()),
      event.args.eligibility,
      Ethereum_Snapshot.deserialize(lastSnapshot.serialize())
    )
    verbose(
      `network-registry: account ${account.to_string()} is ${event.args.eligibility ? 'eligible' : 'not eligible'}`
    )
    // emit event only when eligibility changes on accounts with a HoprNode associated
    try {
      let hoprNodes = await this.db.find_hopr_node_using_account_in_network_registry(
        Ethereum_Address.deserialize(account.serialize())
      )
      let nodes: Address[] = []
      while (hoprNodes.len() > 0) {
        nodes.push(Address.deserialize(hoprNodes.next().serialize()))
      }

      this.emit('network-registry-eligibility-changed', account, nodes, event.args.eligibility)
    } catch (err) {
      log('error while changing eligibility', err)
    }
  }

  private async onRegistered(
    event: RegistryEvent<'Registered'> | RegistryEvent<'RegisteredByOwner'>,
    lastSnapshot: Snapshot
  ): Promise<void> {
    let hoprNode: PeerId
    try {
      hoprNode = peerIdFromString(event.args.hoprPeerId)
    } catch (error) {
      log(`Invalid peer Id '${event.args.hoprPeerId}' given in event 'onRegistered'`)
      log(error)
      return
    }

    assert(lastSnapshot !== undefined)
    const account = Address.from_string(event.args.account)
    await this.db.add_to_network_registry(
      Ethereum_PublicKey.from_peerid_str(hoprNode.toString()).to_address(),
      Ethereum_Address.deserialize(account.serialize()),
      Ethereum_Snapshot.deserialize(lastSnapshot.serialize())
    )
    verbose(`network-registry: node ${event.args.hoprPeerId} is allowed to connect`)
  }

  private async onDeregistered(
    event: RegistryEvent<'Deregistered'> | RegistryEvent<'DeregisteredByOwner'>,
    lastSnapshot: Snapshot
  ): Promise<void> {
    let hoprNode: PeerId
    try {
      hoprNode = peerIdFromString(event.args.hoprPeerId)
    } catch (error) {
      log(`Invalid peer Id '${event.args.hoprPeerId}' given in event 'onDeregistered'`)
      log(error)
      return
    }
    assert(lastSnapshot !== undefined)
    await this.db.remove_from_network_registry(
      Ethereum_PublicKey.from_peerid_str(hoprNode.toString()).to_address(),
      Ethereum_Address.from_string(event.args.account),
      Ethereum_Snapshot.deserialize(lastSnapshot.serialize())
    )
    verbose(`network-registry: node ${event.args.hoprPeerId} is not allowed to connect`)
  }

  private async onEnabledNetworkRegistry(
    event: RegistryEvent<'EnabledNetworkRegistry'>,
    lastSnapshot: Snapshot
  ): Promise<void> {
    assert(lastSnapshot !== undefined)
    this.emit('network-registry-status-changed', event.args.isEnabled)
    await this.db.set_network_registry(event.args.isEnabled, Ethereum_Snapshot.deserialize(lastSnapshot.serialize()))
  }

  private async onTransfer(event: TokenEvent<'Transfer'>, lastSnapshot: Snapshot) {
    const isIncoming = Address.from_string(event.args.to).eq(this.address)
    const amount = new Balance(event.args.value.toString(), BalanceType.HOPR)

    assert(lastSnapshot !== undefined)
    if (isIncoming) {
      await this.db.add_hopr_balance(
        Ethereum_Balance.deserialize(amount.serialize_value(), BalanceType.HOPR),
        Ethereum_Snapshot.deserialize(lastSnapshot.serialize())
      )
    } else {
      await this.db.sub_hopr_balance(
        Ethereum_Balance.deserialize(amount.serialize_value(), BalanceType.HOPR),
        Ethereum_Snapshot.deserialize(lastSnapshot.serialize())
      )
    }
  }

  private indexEvent(indexerEvent: IndexerEvents) {
    log(`Indexer indexEvent ${indexerEvent}`)
    this.emit(indexerEvent)
  }

  public async getAccount(address: Address): Promise<AccountEntry | undefined> {
    let account = await this.db.get_account(Ethereum_Address.deserialize(address.serialize()))
    if (account !== undefined) {
      return AccountEntry.deserialize(account.serialize())
    }

    return account
  }

  public async getPublicKeyOf(address: Address): Promise<PublicKey> {
    const account = await this.getAccount(address)
    if (account !== undefined) {
      return account.public_key
    }
    throw new Error('Could not find public key for address - have they announced? -' + address.to_hex())
  }

  public async *getAddressesAnnouncedOnChain() {
    let announced = await this.db.get_accounts()
    while (announced.len() > 0) {
      yield new Multiaddr(announced.next().get_multiaddress_str())
    }
  }

  public async getPublicNodes(): Promise<{ id: PeerId; multiaddrs: Multiaddr[] }[]> {
    const result: { id: PeerId; multiaddrs: Multiaddr[] }[] = []
    let out = `Known public nodes:\n`

    let publicAccounts = await this.db.get_public_node_accounts()
    while (publicAccounts.len() > 0) {
      let account = publicAccounts.next()

      out += `  - ${account.get_peer_id_str()} ${account.get_multiaddress_str()}\n`
      result.push({
        id: peerIdFromString(account.get_peer_id_str()),
        multiaddrs: [new Multiaddr(account.get_multiaddress_str())]
      })
    }

    // Remove last `\n`
    log(out.substring(0, out.length - 1))

    return result
  }

  /**
   * Returns a random open channel.
   * NOTE: channels with status 'PENDING_TO_CLOSE' are not included
   * @returns an open channel
   */
  public async getRandomOpenChannel(): Promise<ChannelEntry | undefined> {
    const channels = await this.db.get_channels_open()

    if (channels.len() == 0) {
      log('no open channels exist in indexer')
      return undefined
    }

    return ChannelEntry.deserialize(channels.at(random_integer(0, channels.len())).serialize())
  }

  /**
   * Returns peer's open channels.
   * NOTE: channels with status 'PENDING_TO_CLOSE' are not included
   * @param source peer
   * @returns peer's open channels
   */
  public async getOpenChannelsFrom(source: Address): Promise<ChannelEntry[]> {
    let allChannels = await this.db.get_channels_from(source)
    let channels: ChannelEntry[] = []
    while (allChannels.len() > 0) {
      channels.push(ChannelEntry.deserialize(allChannels.next().serialize()))
    }
    return channels.filter((channel) => channel.status === ChannelStatus.Open)
  }

  public resolvePendingTransaction(eventType: IndexerEvents, tx: string): DeferType<string> {
    const deferred = {} as DeferType<string>

    deferred.promise = new Promise<string>((resolve, reject) => {
      let done = false
      let timer: any

      deferred.reject = () => {
        if (done) {
          return
        }
        timer?.clear()
        done = true

        this.removeListener(eventType, deferred.resolve)
        log('listener %s on %s is removed due to error', eventType, tx)
        setImmediate(resolve, tx)
      }

      timer = retimer(
        () => {
          if (done) {
            return
          }
          timer?.clear()
          done = true
          // remove listener but throw now error
          this.removeListener(eventType, deferred.resolve)
          log('listener %s on %s timed out and thus removed', eventType, tx)
          setImmediate(reject, tx)
        },
        constants.INDEXER_TIMEOUT,
        `Timeout while indexer waiting for confirming transaction ${tx}`
      )

      deferred.resolve = () => {
        if (done) {
          return
        }
        timer?.clear()
        done = true

        this.removeListener(eventType, deferred.resolve)
        log('listener %s on %s is resolved and thus removed', eventType, tx)

        setImmediate(resolve, tx)
      }

      this.addListener(eventType, deferred.resolve)
      log('listener %s on %s is added', eventType, tx)
    })

    return deferred
  }
}

export default Indexer
