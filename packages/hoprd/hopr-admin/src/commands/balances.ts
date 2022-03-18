import type API from '../utils/api'
import { toPaddedString } from '../utils'
import { Command } from '../utils/command'
import { utils as ethersUtils } from 'ethers'

export default class Balances extends Command {
  constructor(api: API, extra: { getCachedAliases: () => Record<string, string> }) {
    super(
      {
        default: [[], 'shows all balances'],
        onlyOne: [[['hoprOrNative', 'type', false]], 'shows shows one balance']
      },
      api,
      extra
    )
  }

  public name() {
    return 'balance'
  }

  public description() {
    return 'Displays your current HOPR and native balance'
  }

  /**
   * Prints the balance of our account.
   * @notice triggered by the CLI
   */
  public async execute(log, query): Promise<void> {
    const [error, use, type] = this.assertUsage(query) as [string | undefined, string, string]
    if (error) return log(error)

    const balances = await this.api.getBalances()
    const hoprPrefix = 'HOPR Balance:'
    const hoprBalance = ethersUtils.formatEther(balances.hopr)
    const nativePrefix = 'Native Balance:'
    const nativeBalance = ethersUtils.formatEther(balances.native)

    if (use === 'onlyOne') {
      if (type === 'hopr') {
        return log(toPaddedString([[hoprPrefix, hoprBalance]]))
      } else {
        return log(toPaddedString([[nativePrefix, nativeBalance]]))
      }
    } else {
      return log(
        toPaddedString([
          [hoprPrefix, hoprBalance],
          [nativePrefix, nativeBalance]
        ])
      )
    }
  }
}
