import type { TransactionRequest } from '@ethersproject/abstract-provider'
import { debug } from '@hoprnet/hopr-utils'
import { BigNumber } from 'ethers'
import { isDeepStrictEqual } from 'util'
const log = debug('hopr-core-ethereum:transcation-manager')

export type TransactionPayload = {
  to: string
  data: string
  value: BigNumber
}
export type Transaction = {
  nonce: number
  createdAt: number
  maxPrority: BigNumber
}

/**
 * Keep track of queuing, pending, mined and confirmed transactions,
 * and allows for pruning unnecessary data.
 * After signing, a tx is `queuing`.
 * After being broadcasted, a tx is `pending`.
 * After being included in a block, a tx is `mined`.
 * After passing the finality threshold, a tx is `confirmed`.
 * This class is mainly used by nonce-tracker which relies
 * on transcation-manager to keep an update to date view
 * on transactions.
 */
class TranscationManager {
  /**
   * transaction payloads
   */
  public readonly payloads = new Map<string, TransactionPayload>()
  /**
   * transaction requests, before signing
   */
  public readonly queuing = new Map<string, Transaction>()
  /**
   * pending transactions
   */
  public readonly pending = new Map<string, Transaction>()
  /**
   * mined transactions
   */
  public readonly mined = new Map<string, Transaction>()
  /**
   * confirmed transactions
   */
  public readonly confirmed = new Map<string, Transaction>()

  /**
   * Return all the queuing transactions
   * @returns Array of transaction hashes
   */
  public getAllQueuingTxs(): TransactionRequest[] {
    // queuing tx hashes
    const queuingTxHash = Array.from(this.queuing.keys())
    return queuingTxHash.map((txHash) => {
      const { to, data, value } = this.payloads.get(txHash)
      const { nonce, maxPrority } = this.queuing.get(txHash)
      return {
        to,
        data,
        value,
        nonce,
        maxPrority
      }
    })
  }

  /**
   * Return pending and mined transactions
   * @returns Array of transaction hashes
   */
  public getAllUnconfirmedTxs(): Transaction[] {
    return Array.from(this.pending.values()).concat(Array.from(this.mined.values()))
  }

  /**
   * Return pending and mined transactions
   * @returns Array of transaction hashes
   */
  public getAllUnconfirmedHash(): string[] {
    return Array.from(this.pending.keys()).concat(Array.from(this.mined.keys()))
  }

  /**
   * If a transaction payload exists in mined or pending with a higher/equal gas price
   * @param payload object
   * @param maxPrority Max Priority Fee. Tips paying to the miners, which correlates to the likelyhood of getting transactions included.
   * @returns [true if it exists, transaction hash]
   */
  public existInMinedOrPendingWithHigherFee(payload: TransactionPayload, maxPrority: BigNumber): [boolean, string] {
    // Using isDeepStrictEqual to compare TransactionPayload objects, see
    // https://nodejs.org/api/util.html#util_util_isdeepstrictequal_val1_val2
    const index = Array.from(this.payloads.values()).findIndex((pl) => isDeepStrictEqual(pl, payload))
    if (index < 0) {
      return [false, '']
    }

    const hash = Array.from(this.payloads.keys())[index]
    if (
      !this.mined.get(hash) &&
      BigNumber.from((this.pending.get(hash) ?? this.queuing.get(hash)).maxPrority).lt(maxPrority)
    ) {
      return [false, hash]
    }
    return [true, hash]
  }

  /**
   * Adds queuing transaction
   * @param hash transaction hash
   * @param transaction object
   * @returns true if transaction got added to queue, otherwise false
   */
  public addToQueuing(
    hash: string,
    transaction: Omit<Transaction, 'createdAt'>,
    transactionPayload: TransactionPayload
  ): boolean {
    if (this.queuing.has(hash)) {
      return false
    }

    log('Adding queuing transaction %s %i', hash, transaction.nonce)
    this.payloads.set(hash, transactionPayload)
    this.queuing.set(hash, { nonce: transaction.nonce, createdAt: 0, maxPrority: transaction.maxPrority })

    return true
  }

  /**
   * Moves transaction from queuing to pending
   * @param hash transaction hash
   */
  public moveFromQueuingToPending(hash: string): void {
    if (!this.queuing.has(hash)) return

    log('Moving transaction to pending %s', hash)
    this.pending.set(hash, { ...this.queuing.get(hash), createdAt: this._getTime() })
    this.queuing.delete(hash)
  }

  /**
   * Moves transcation from queuing or pending or mined to confirmed
   * @param hash transaction hash
   */
  public moveToConfirmed(hash: string): void {
    if (this.queuing.has(hash)) {
      this.moveFromQueuingToPending(hash)
    }
    if (this.pending.has(hash)) {
      this.moveFromPendingToMined(hash)
    }
    if (this.mined.has(hash)) {
      this.moveFromMinedToConfirmed(hash)
    }
    return
  }

  /**
   * Moves transcation from pending to mined
   * @param hash transaction hash
   */
  public moveFromPendingToMined(hash: string): void {
    if (!this.pending.has(hash)) return

    log('Moving transaction to mined %s', hash)
    this.mined.set(hash, this.pending.get(hash))
    this.pending.delete(hash)
  }

  /**
   * Moves transcation from mined to confirmed. Delete payload
   * @param hash transaction hash
   */
  public moveFromMinedToConfirmed(hash: string): void {
    if (!this.mined.has(hash)) return

    log('Moving transaction to confirmed %s', hash)
    this.confirmed.set(hash, this.mined.get(hash))
    this.mined.delete(hash)
    this.payloads.delete(hash)
  }

  /**
   * Removed transcation from queuing, pending, mined and confirmed
   * @param hash transaction hash
   */
  public remove(hash: string): void {
    log('Removing transaction %s', hash)
    this.payloads.delete(hash)
    this.queuing.delete(hash)
    this.pending.delete(hash)
    this.mined.delete(hash)
    this.confirmed.delete(hash)
  }

  /**
   * Removes confirmed blocks except last 5 nonces.
   * This is a way for us to clean up some memory which we know
   * we don't need anymore.
   */
  public prune(): void {
    const descTxs = Array.from(this.confirmed.entries()).sort(([, a], [, b]) => {
      return b.nonce - a.nonce
    })

    for (const [hash] of descTxs.slice(5, descTxs.length)) {
      this.remove(hash)
    }
  }

  /**
   * @returns current timestamp
   */
  private _getTime(): number {
    return new Date().getTime()
  }
}

export default TranscationManager
