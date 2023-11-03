import request from 'supertest'
import sinon from 'sinon'
import chaiResponseValidator from 'chai-openapi-response-validator'
import chai, { expect } from 'chai'
import {
  createTestApiInstance,
  ALICE_PEER_ID,
  ALICE_MULTI_ADDR,
  ALICE_ETHEREUM_ADDR,
  BOB_PEER_ID,
  BOB_MULTI_ADDR,
  BOB_ETHEREUM_ADDR
} from '../../fixtures.js'
import { STATUS_CODES } from '../../utils.js'
import type { Hopr } from '@hoprnet/hopr-utils'

const ALICE_ENTRY_INFO = {
  id: ALICE_PEER_ID.toString(),
  address: ALICE_ETHEREUM_ADDR,
  multiaddrs: [ALICE_MULTI_ADDR.toString()]
}

const BOB_ENTRY_INFO = {
  id: BOB_PEER_ID.toString(),
  address: BOB_ETHEREUM_ADDR,
  multiaddrs: [BOB_MULTI_ADDR.toString()]
}

let node = sinon.fake() as any as Hopr
node.isAllowedToAccessNetwork = (peer: string) => {
  switch (peer) {
    case ALICE_PEER_ID.toString():
      return Promise.resolve(false)
    case BOB_PEER_ID.toString():
      return Promise.resolve(true)
  }
}

describe('GET /node/entryNodes', function () {
  let service: any
  before(async function () {
    const loaded = await createTestApiInstance(node)

    service = loaded.service

    // @ts-ignore ESM / CommonJS compatibility issue
    chai.use(chaiResponseValidator.default(loaded.api.apiDoc))
  })

  it('should return invalid quality when quality is not a number', async function () {
    node.getPublicNodes = sinon.fake.resolves([ALICE_ENTRY_INFO, BOB_ENTRY_INFO])
    const res = await request(service).get(`/api/v3/node/entryNodes`).send()
    expect(res.status).to.equal(200)
    expect(res).to.satisfyApiSpec
    expect(res.body).to.deep.equal({
      [ALICE_PEER_ID.toString()]: {
        multiaddrs: [ALICE_MULTI_ADDR.toString()],
        isEligible: false
      },
      [BOB_PEER_ID.toString()]: {
        multiaddrs: [BOB_MULTI_ADDR.toString()],
        isEligible: true
      }
    })
  })

  it('should handle error', async function () {
    node.getPublicNodes = () => {
      throw Error(`boom`)
    }

    const res = await request(service).get(`/api/v3/node/entryNodes`).send()
    expect(res.status).to.equal(422)
    expect(res.body.status).to.equal(STATUS_CODES.UNKNOWN_FAILURE)
  })
})
