import chalk from 'chalk'

import type HoprCoreConnector from '@hoprnet/hopr-core-connector-interface'
import type { Channel as ChannelInstance } from '@hoprnet/hopr-core-connector-interface'
import type Hopr from '../../src'

import AbstractCommand from './abstractCommand'


import { u8aToHex, pubKeyToPeerId } from '../../src/utils'


export default class ListOpenChannels implements AbstractCommand {
    constructor(public node: Hopr<HoprCoreConnector>) { }
    /**
     * Lists all channels that we have with other nodes. Triggered from the CLI.
     */
    async execute(): Promise<void> {
        let str = `${chalk.yellow('ChannelId:'.padEnd(66, ' '))} - ${chalk.blue('PeerId:')}\n`

        try {
            await this.node.paymentChannels.channel.getAll(
                this.node.paymentChannels,
                async (channel: ChannelInstance<HoprCoreConnector>) => {
                    const channelId = await channel.channelId
                    if (!channel.counterparty) {
                        str += `${chalk.yellow(u8aToHex(channelId))} - ${chalk.gray('pre-opened')}`
                        return
                    }

                    const peerId = await pubKeyToPeerId(await channel.offChainCounterparty)

                    str += `${chalk.yellow(u8aToHex(channelId))} - ${chalk.blue(peerId.toB58String())}\n`
                    return
                },
                async (promises: Promise<void>[]) => {
                    if (promises.length == 0) {
                        str += `  No open channels.`
                        return
                    }

                    await Promise.all(promises)

                    return
                }
            )
            console.log(str)
            return
        } catch (err) {
            console.log(chalk.red(err.message))
            return
        }
    }

    complete(line: string, cb: (err: Error | undefined, hits: [string[], string]) => void): void {
        cb(undefined, [[''], line])
    }
}

