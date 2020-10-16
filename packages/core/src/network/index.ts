import type { HoprOptions } from '..'
import type { Interactions } from '../interactions'
import type { LibP2P } from '../index'
import { Crawler } from './crawler'
import Heartbeat from './heartbeat'
import PeerStore from './peerStore'
import Stun from './stun'
import Multiaddr from 'multiaddr'
import PeerId from 'peer-id'
import PeerInfo from 'peer-info'

type TestOpts = {
  crawl?: { timeoutIntentionally?: boolean }
}
class Network {
  public crawler: Crawler
  public heartbeat: Heartbeat
  public peerStore: PeerStore
  public stun?: Stun

  constructor(node: LibP2P, interactions: Interactions<any>, private options: HoprOptions, testingOptions?: TestOpts) {

    // These are temporary, and will be replaced by accessors to the addressBook
    const putPeer = (ma: Multiaddr) => {
      const pinfo = new PeerInfo(PeerId.createFromB58String(ma.getPeerId()))
      pinfo.multiaddrs.add(ma)
      node.peerStore.put(pinfo)
    }
    const getPeer = (id: PeerId): Multiaddr[] => {
      return node.peerStore.get(id).multiaddrs.toArray()
    }

    this.peerStore = new PeerStore(node.peerStore.peers.values())
    this.heartbeat = new Heartbeat(this.peerStore, interactions.network.heartbeat, node.hangUp)
    this.crawler = new Crawler(
      node.peerInfo.id,
      this.peerStore,
      interactions.network.crawler,
      getPeer,
      putPeer,
      testingOptions?.crawl
    )

    node.on('peer:connect', (peerInfo: PeerInfo) => {
      this.peerStore.onPeerConnect(peerInfo)
      this.heartbeat.connectionListener(peerInfo)
    })

    if (options.bootstrapNode) {
      this.stun = new Stun(options.hosts)
    }
  }


  async start() {
    if (this.options.bootstrapNode) {
      await this.stun?.startServer()
    }

    this.heartbeat?.start()
  }

  async stop() {
    if (this.options.bootstrapNode) {
      await this.stun?.stopServer()
    }

    this.heartbeat?.stop()
  }
}

export { Network }
