import asyncio
import multiprocessing
import os
import random
import re
import socket
import string
from contextlib import AsyncExitStack, asynccontextmanager, contextmanager

import pytest
import requests

from .conftest import (
    API_TOKEN,
    OPEN_CHANNEL_FUNDING_VALUE_HOPR,
    RESERVED_TAG_UPPER_BOUND,
    TICKET_AGGREGATION_THRESHOLD,
    TICKET_PRICE_PER_HOP,
    barebone_nodes,
    default_nodes,
    nodes_with_auth,
    random_distinct_pairs_from,
)
from .hopr import HoprdAPI
from .node import Node

PARAMETERIZED_SAMPLE_SIZE = 1  # if os.getenv("CI", default="false") == "false" else 3
AGGREGATED_TICKET_PRICE = TICKET_AGGREGATION_THRESHOLD * TICKET_PRICE_PER_HOP
MULTIHOP_MESSAGE_SEND_TIMEOUT = 30.0
CHECK_RETRY_INTERVAL = 0.5
APPLICATION_TAG_THRESHOLD_FOR_SESSIONS = RESERVED_TAG_UPPER_BOUND + 1


def shuffled(coll):
    random.shuffle(coll)
    return coll


@asynccontextmanager
async def create_channel(src: Node, dest: Node, funding: int, close_from_dest=True):
    channel = await src.api.open_channel(dest.address, str(int(funding)))
    assert channel is not None
    await asyncio.wait_for(check_channel_status(src, dest, status="Open"), 10.0)
    try:
        yield channel
    finally:
        if close_from_dest:
            assert await dest.api.close_channel(channel)
            await asyncio.wait_for(check_channel_status(src, dest, status="Closed"), 10.0)
        else:
            assert await src.api.close_channel(channel)
            await asyncio.wait_for(check_channel_status(src, dest, status="PendingToClose"), 10.0)

            # need to wait some more time until closure time has passed and the
            # closure may be finalized
            await asyncio.sleep(15)

            assert await src.api.close_channel(channel)
            await asyncio.wait_for(check_channel_status(src, dest, status="Closed"), 10.0)


async def get_channel(src: Node, dest: Node, include_closed=False):
    all_channels = await src.api.all_channels(include_closed=include_closed)

    channels = [
        oc for oc in all_channels.all if oc.source_address == src.address and oc.destination_address == dest.address
    ]

    return channels[0] if len(channels) > 0 else None


async def get_channel_seen_from_dst(src: Node, dest: Node, include_closed=False):
    open_channels = await dest.api.all_channels(include_closed=include_closed)
    channels = [
        oc for oc in open_channels.all if oc.source_address == src.address and oc.destination_address == dest.address
    ]

    return channels[0] if len(channels) > 0 else None


async def check_channel_status(src: Node, dest: Node, status: str):
    assert status in ["Open", "PendingToClose", "Closed"]
    include_closed = status == "Closed"
    while True:
        channel = await get_channel(src, dest, include_closed)
        channel_seen_from_dst = await get_channel_seen_from_dst(src, dest, include_closed)
        if (
            channel is not None
            and channel.status == status
            and channel_seen_from_dst is not None
            and channel_seen_from_dst.status == status
        ):
            break
        else:
            await asyncio.sleep(CHECK_RETRY_INTERVAL)


async def check_outgoing_channel_closed(src: Node, channel_id: str):
    while True:
        channel = await src.api.get_channel(channel_id)
        if channel is not None and channel.status == "Closed":
            break
        else:
            await asyncio.sleep(CHECK_RETRY_INTERVAL)


async def check_received_packets_with_pop(receiver: Node, expected_packets, tag=None, sort=True):
    received = []

    while len(received) != len(expected_packets):
        packet = await receiver.api.messages_pop(tag)
        if packet is not None:
            received.append(packet.body)
        else:
            await asyncio.sleep(CHECK_RETRY_INTERVAL)

    if sort:
        expected_packets.sort()
        received.sort()

    assert received == expected_packets


async def check_received_packets_with_peek(receiver: Node, expected_packets: list[str], tag=None, sort=True):
    received = []

    while len(received) != len(expected_packets):
        packets = await receiver.api.messages_peek_all(tag)

        if packets is None:
            await asyncio.sleep(CHECK_RETRY_INTERVAL)
            continue

        received = [m.body for m in packets.messages]

    if sort:
        expected_packets.sort()
        received.sort()

    assert received == expected_packets, f"Expected: {expected_packets}, got: {received}"


async def check_rejected_tickets_value(src: Node, value: int):
    while balance_str_to_int((await src.api.get_tickets_statistics()).rejected_value) < value:
        await asyncio.sleep(CHECK_RETRY_INTERVAL)


async def check_unredeemed_tickets_value(src: Node, value: int):
    while balance_str_to_int((await src.api.get_tickets_statistics()).unredeemed_value) < value:
        await asyncio.sleep(CHECK_RETRY_INTERVAL)


async def check_safe_balance(src: Node, value: int):
    while balance_str_to_int((await src.api.balances()).safe_hopr) != value:
        await asyncio.sleep(CHECK_RETRY_INTERVAL)


async def check_native_balance_below(src: Node, value: int):
    while balance_str_to_int((await src.api.balances()).native) >= value:
        await asyncio.sleep(CHECK_RETRY_INTERVAL)


async def check_all_tickets_redeemed(src: Node):
    while balance_str_to_int((await src.api.get_tickets_statistics()).unredeemed_value) > 0:
        await asyncio.sleep(CHECK_RETRY_INTERVAL)


async def send_and_receive_packets_with_pop(
    packets, src: Node, dest: Node, path: str, timeout: int = MULTIHOP_MESSAGE_SEND_TIMEOUT
):
    random_tag = random.randint(APPLICATION_TAG_THRESHOLD_FOR_SESSIONS, 65530)

    for packet in packets:
        assert await src.api.send_message(dest.peer_id, packet, path, random_tag)

    await asyncio.wait_for(check_received_packets_with_pop(dest, packets, tag=random_tag, sort=True), timeout)


async def send_and_receive_packets_with_peek(
    packets, src: Node, dest: Node, path: str, timeout: int = MULTIHOP_MESSAGE_SEND_TIMEOUT
):
    random_tag = random.randint(APPLICATION_TAG_THRESHOLD_FOR_SESSIONS, 65530)

    for packet in packets:
        assert await src.api.send_message(dest.peer_id, packet, path, random_tag)

    await asyncio.wait_for(check_received_packets_with_peek(dest, packets, tag=random_tag, sort=True), timeout)

    return random_tag


def balance_str_to_int(balance: str):
    return int(balance.split(" ", 1)[0])


# NOTE: this test is first, ensuring that all tests following it have ensured connectivity and
# correct ticket price from api
@pytest.mark.asyncio
async def test_hoprd_swarm_connectivity(swarm7: dict[str, Node]):
    async def check_all_connected(me: Node, others: list[str]):
        others2 = set(others)
        while True:
            current_peers = set([x["peer_id"] for x in await me.api.peers()])
            if current_peers.intersection(others) == others2:
                break
            else:
                assert current_peers.intersection(others2) == others2
                await asyncio.sleep(0.5)

    await asyncio.gather(
        *[
            asyncio.wait_for(
                check_all_connected(swarm7[k], [swarm7[v].peer_id for v in barebone_nodes() if v != k]), 60.0
            )
            for k in barebone_nodes()
        ]
    )

    ticket_price = await random.choice(list(swarm7.values())).api.ticket_price()
    if ticket_price is not None:
        global TICKET_PRICE_PER_HOP, AGGREGATED_TICKET_PRICE
        TICKET_PRICE_PER_HOP = ticket_price
        AGGREGATED_TICKET_PRICE = TICKET_AGGREGATION_THRESHOLD * TICKET_PRICE_PER_HOP
    else:
        print("Could not get ticket price from API, using default value")


@pytest.mark.parametrize("peer", random.sample(nodes_with_auth(), 1))
def test_hoprd_rest_api_should_reject_connection_without_any_auth(swarm7: dict[str, Node], peer: str):
    url = f"http://{swarm7[peer].host_addr}:{swarm7[peer].api_port}/api/v3/node/version"

    r = requests.get(url)

    assert r.status_code == 401


@pytest.mark.parametrize("peer", random.sample(nodes_with_auth(), 1))
def test_hoprd_rest_api_should_reject_connection_with_invalid_token(peer: str, swarm7: dict[str, Node]):
    url = f"http://{swarm7[peer].host_addr}:{swarm7[peer].api_port}/api/v3/node/version"
    headers = {"X-Auth-Token": "DefiNItEly_A_baD_TokEn"}

    r = requests.get(url, headers=headers)

    assert r.status_code == 401


@pytest.mark.parametrize("peer", random.sample(nodes_with_auth(), 1))
def test_hoprd_rest_api_should_accept_connection_with_valid_token(peer: str, swarm7: dict[str, Node]):
    url = f"http://{swarm7[peer].host_addr}:{swarm7[peer].api_port}/api/v3/node/version"
    headers = {"X-Auth-Token": f"{API_TOKEN}"}

    r = requests.get(url, headers=headers)

    assert r.status_code == 200


@pytest.mark.asyncio
async def test_hoprd_protocol_check_balances_without_prior_tests(swarm7: dict[str, Node]):
    for node in swarm7.values():
        addr = await node.api.addresses("native")
        assert re.match("^0x[0-9a-fA-F]{40}$", addr) is not None
        balances = await node.api.balances()
        native_balance = int(balances.native.split(" ")[0])
        hopr_balance = int(balances.safe_hopr.split(" ")[0])
        assert native_balance > 0
        assert hopr_balance > 0


@pytest.mark.asyncio
@pytest.mark.parametrize("peer", random.sample(barebone_nodes(), 1))
async def test_hoprd_node_should_be_able_to_alias_other_peers(peer: str, swarm7: dict[str, Node]):
    other_peers = barebone_nodes()
    other_peers.remove(peer)

    alice_peer_id = swarm7[random.choice(other_peers)].peer_id
    my_peer_id = swarm7[peer].peer_id
    assert alice_peer_id != my_peer_id

    assert await swarm7[peer].api.aliases_get_alias("me") == my_peer_id

    assert await swarm7[peer].api.aliases_get_alias("Alice") is None
    assert await swarm7[peer].api.aliases_set_alias("Alice", alice_peer_id) is True

    assert await swarm7[peer].api.aliases_get_alias("Alice") == alice_peer_id
    assert await swarm7[peer].api.aliases_set_alias("Alice", alice_peer_id) is False

    assert await swarm7[peer].api.aliases_remove_alias("Alice")
    assert await swarm7[peer].api.aliases_get_alias("Alice") is None


@pytest.mark.asyncio
@pytest.mark.parametrize("src, dest", random_distinct_pairs_from(barebone_nodes(), count=PARAMETERIZED_SAMPLE_SIZE))
async def test_hoprd_ping_should_work_between_nodes_in_the_same_network(src: str, dest: str, swarm7: dict[str, Node]):
    response = await swarm7[src].api.ping(swarm7[dest].peer_id)

    assert response is not None
    assert int(response.latency) > 0, f"Non-0 round trip time expected, actual: '{int(response.latency)}'"


@pytest.mark.asyncio
@pytest.mark.parametrize("peer", random.sample(barebone_nodes(), 1))
async def test_hoprd_ping_to_self_should_fail(peer: str, swarm7: dict[str, Node]):
    response = await swarm7[peer].api.ping(swarm7[peer].peer_id)

    assert response is None, f"Pinging self should fail"


@pytest.mark.asyncio
async def test_hoprd_ping_should_not_be_able_to_ping_nodes_in_other_network_UNFINISHED(swarm7: dict[str, Node]):
    """
    # FIXME: re-enable when network check works
    # log "Node 1 should not be able to talk to Node 6 (different network id)"
    # result=$(api_ping "${api6}" ${addr1} "TIMEOUT")
    # log "-- ${result}"

    # FIXME: re-enable when network check works
    # log "Node 6 should not be able to talk to Node 1 (different network id)"
    # result=$(api_ping "${api6}" ${addr1} "TIMEOUT")
    # log "-- ${result}"
    """
    assert True


@pytest.mark.asyncio
async def test_hoprd_ping_should_not_be_able_to_ping_nodes_not_present_in_the_registry_UNFINISHED(
    swarm7: dict[str, Node],
):
    """
    # log "Node 7 should not be able to talk to Node 1 (Node 7 is not in the register)"
    # result=$(ping "${api7}" ${addr1} "TIMEOUT")
    # log "-- ${result}"

    # log "Node 1 should not be able to talk to Node 7 (Node 7 is not in the register)"
    # result=$(ping "${api1}" ${addr7} "TIMEOUT")
    # log "-- ${result}"
    """
    assert True


@pytest.mark.asyncio
@pytest.mark.parametrize("peer", barebone_nodes())
async def test_hoprd_should_not_have_unredeemed_tickets_without_sending_messages(peer: str, swarm7: dict[str, Node]):
    statistics = await swarm7[peer].api.get_tickets_statistics()

    assert balance_str_to_int(statistics.unredeemed_value) == 0


@pytest.mark.asyncio
@pytest.mark.parametrize("src, dest", random_distinct_pairs_from(barebone_nodes(), count=PARAMETERIZED_SAMPLE_SIZE))
async def test_hoprd_should_be_able_to_send_0_hop_messages_without_open_channels(
    src: Node, dest: Node, swarm7: dict[str, Node]
):
    message_count = int(TICKET_AGGREGATION_THRESHOLD / 10)

    packets = [f"0 hop message #{i:08d}" for i in range(message_count)]
    await send_and_receive_packets_with_pop(packets, src=swarm7[src], dest=swarm7[dest], path=[])


@pytest.mark.asyncio
@pytest.mark.parametrize("src, dest", random_distinct_pairs_from(barebone_nodes(), count=PARAMETERIZED_SAMPLE_SIZE))
async def test_hoprd_should_fail_sending_a_message_that_is_too_large(src: Node, dest: Node, swarm7: dict[str, Node]):
    MAXIMUM_PAYLOAD_SIZE = 500
    random_tag = random.randint(APPLICATION_TAG_THRESHOLD_FOR_SESSIONS, 65530)

    packet = "0 hop message too large: " + "".join(
        random.choices(string.ascii_uppercase + string.digits, k=MAXIMUM_PAYLOAD_SIZE)
    )
    assert await swarm7[src].api.send_message(swarm7[dest].peer_id, packet, [], random_tag) == None


@pytest.mark.asyncio
@pytest.mark.parametrize("src,dest", [tuple(shuffled(barebone_nodes())[:2]) for _ in range(PARAMETERIZED_SAMPLE_SIZE)])
async def test_hoprd_api_channel_should_register_fund_increase_using_fund_endpoint(
    src: str, dest: str, swarm7: dict[str, Node]
):
    hopr_amount = f"{OPEN_CHANNEL_FUNDING_VALUE_HOPR * 1e18:.0f}"  # convert HOPR to weiHOPR

    async with create_channel(swarm7[src], swarm7[dest], funding=TICKET_PRICE_PER_HOP) as channel:
        balance_before = await swarm7[src].api.balances()
        channel_before = await swarm7[src].api.get_channel(channel)

        assert await swarm7[src].api.channels_fund_channel(channel, hopr_amount)

        channel_after = await swarm7[src].api.get_channel(channel)

        # Updated channel balance is visible immediately
        assert balance_str_to_int(channel_after.balance) - balance_str_to_int(
            channel_before.balance
        ) == balance_str_to_int(hopr_amount)

        # Wait until the safe balance has decreased
        await asyncio.wait_for(
            check_safe_balance(
                swarm7[src], balance_str_to_int(balance_before.safe_hopr) - balance_str_to_int(hopr_amount)
            ),
            20.0,
        )

        # Safe allowance can be checked too at this point
        balance_after = await swarm7[src].api.balances()
        assert balance_str_to_int(balance_before.safe_hopr_allowance) - balance_str_to_int(
            balance_after.safe_hopr_allowance
        ) == balance_str_to_int(hopr_amount)

        await asyncio.wait_for(check_native_balance_below(swarm7[src], balance_str_to_int(balance_before.native)), 20.0)


@pytest.mark.asyncio
@pytest.mark.parametrize("src,dest", [tuple(shuffled(barebone_nodes())[:2]) for _ in range(PARAMETERIZED_SAMPLE_SIZE)])
async def test_hoprd_api_should_redeem_tickets_in_channel_using_redeem_endpoint(
    src: Node, dest: Node, swarm7: dict[str, Node]
):
    message_count = 2

    async with create_channel(
        swarm7[src], swarm7[dest], funding=message_count * TICKET_PRICE_PER_HOP, close_from_dest=False
    ) as channel:
        packets = [f"Channel redeem on 1-hop: {src} - {dest} - {src} #{i:08d}" for i in range(message_count)]

        await send_and_receive_packets_with_pop(packets, src=swarm7[src], dest=swarm7[src], path=[swarm7[dest].peer_id])

        await asyncio.wait_for(check_unredeemed_tickets_value(swarm7[dest], message_count * TICKET_PRICE_PER_HOP), 30.0)

        async def channel_redeem_tickets(api: HoprdAPI, channel: str):
            while True:
                if await api.channel_redeem_tickets(channel):
                    break
                else:
                    await asyncio.sleep(0.5)

        await asyncio.wait_for(channel_redeem_tickets(swarm7[dest].api, channel), 20.0)

        await asyncio.wait_for(check_all_tickets_redeemed(swarm7[dest]), 120.0)

        assert await swarm7[dest].api.channel_get_tickets(channel) == []


@pytest.mark.asyncio
@pytest.mark.parametrize("src,dest", [tuple(shuffled(barebone_nodes())[:2]) for _ in range(PARAMETERIZED_SAMPLE_SIZE)])
async def test_hoprd_should_fail_sending_a_message_when_the_channel_is_out_of_funding(
    src: str, dest: Node, swarm7: dict[str, Node]
):
    """
    # FIXME: The following part can be enabled once incoming channel closure is
    # implemented.
    #
    # need to close the incoming side to not have to wait for the closure timeout
    # api_close_channel "${second_node_id}" "${node_id}" "${second_node_api}" "${node_addr}" "incoming"

    # only fund for 2 tickets
    # channel_info=$(api_open_channel "${node_id}" "${second_node_id}" "${node_api}" "${second_node_addr}" "200")

    # need to wait a little to allow the other side to index the channel open event
    # sleep 10
    # api_get_tickets_in_channel ${second_node_api} ${channel_id} "TICKETS_NOT_FOUND"
    # for i in `seq 1 ${generated_tickets}`; do
    #   log "PendingBalance in channel: Node ${node_id} send 1 hop message to self via node ${second_node_id}"
    #   api_send_message "${node_api}" "${msg_tag}" "${peer_id}" \
    #       "pendingbalance: hello, world 1 self" "${second_peer_id}"
    # done

    # seems like there's slight delay needed for tickets endpoint to return up to date tickets, \
    #       probably because of blockchain sync delay
    # sleep 5

    # ticket_amount=$(api_get_tickets_in_channel ${second_node_api} ${channel_id} | jq '. | length')
    # if [[ "${ticket_amount}" != "${generated_tickets}" ]]; then
    #   msg "PendingBalance: Ticket amount ${ticket_amount} is different than expected ${generated_tickets}"
    #   exit 1
    # fi

    # api_redeem_tickets_in_channel ${second_node_api} ${channel_id}
    # sleep 5
    # api_get_tickets_in_channel ${second_node_api} ${channel_id} "TICKETS_NOT_FOUND"
    # api_close_channel "${node_id}" "${second_node_id}" "${node_api}" "${second_node_addr}" "outgoing"
    """

    message_count = 2

    async with AsyncExitStack() as channels:
        await asyncio.gather(
            *[
                channels.enter_async_context(
                    create_channel(
                        swarm7[i[0]], swarm7[i[1]], funding=message_count * TICKET_PRICE_PER_HOP, close_from_dest=False
                    )
                )
                for i in [[src, dest]]
            ]
        )

        packets = [f"Channel agg and redeem on 1-hop: {src} - {dest} - {src} #{i:08d}" for i in range(message_count)]
        await send_and_receive_packets_with_pop(packets, src=swarm7[src], dest=swarm7[src], path=[swarm7[dest].peer_id])

        # this message has no funding in the channel, but it still should be sent
        assert await swarm7[src].api.send_message(
            swarm7[src].peer_id, "THIS MSG IS NOT COVERED", [swarm7[dest].peer_id]
        )

        await asyncio.wait_for(check_unredeemed_tickets_value(swarm7[dest], message_count * TICKET_PRICE_PER_HOP), 30.0)

        # we should see the last message as rejected
        await asyncio.wait_for(check_rejected_tickets_value(swarm7[dest], 1), 120.0)

        await asyncio.sleep(10)  # wait for aggregation to finish
        assert await swarm7[dest].api.tickets_redeem()

        await asyncio.wait_for(check_all_tickets_redeemed(swarm7[dest]), 120.0)


@pytest.mark.asyncio
@pytest.mark.parametrize("src,dest", [tuple(shuffled(barebone_nodes())[:2]) for _ in range(PARAMETERIZED_SAMPLE_SIZE)])
async def test_hoprd_should_create_redeemable_tickets_on_routing_in_1_hop_to_self_scenario(
    src: str, dest: str, swarm7: dict[str, Node]
):
    # send 90% of messages before ticket aggregation would kick in
    message_count = int(TICKET_AGGREGATION_THRESHOLD / 10 * 9)

    async with create_channel(
        swarm7[src], swarm7[dest], funding=message_count * TICKET_PRICE_PER_HOP, close_from_dest=False
    ) as channel_id:
        # ensure ticket stats are what we expect before starting
        statistics_before = await swarm7[dest].api.get_tickets_statistics()
        assert balance_str_to_int(statistics_before.unredeemed_value) == 0

        packets = [
            f"1 hop message to self: {src} - {dest} - {src} #{i:08d} of #{message_count:08d}"
            for i in range(message_count)
        ]
        await send_and_receive_packets_with_pop(
            packets, src=swarm7[src], dest=swarm7[src], path=[swarm7[dest].peer_id], timeout=60.0
        )

        await asyncio.wait_for(check_unredeemed_tickets_value(swarm7[dest], message_count * TICKET_PRICE_PER_HOP), 30.0)

        # ensure ticket stats are updated after messages are sent
        statistics_after = await swarm7[dest].api.get_tickets_statistics()

        unredeemed_value = balance_str_to_int(statistics_after.unredeemed_value) - balance_str_to_int(
            statistics_before.unredeemed_value
        )

        assert statistics_after.redeemed_value == statistics_before.redeemed_value
        assert unredeemed_value == (len(packets) * TICKET_PRICE_PER_HOP)

        assert await swarm7[dest].api.channel_redeem_tickets(channel_id)

        await asyncio.wait_for(check_all_tickets_redeemed(swarm7[dest]), 120.0)

        # ensure ticket stats are updated after redemption
        statistics_after_redemption = await swarm7[dest].api.get_tickets_statistics()
        assert (
            balance_str_to_int(statistics_after_redemption.redeemed_value)
            - balance_str_to_int(statistics_after.redeemed_value)
        ) == (len(packets) * TICKET_PRICE_PER_HOP)
        assert balance_str_to_int(statistics_after_redemption.unredeemed_value) == 0


@pytest.mark.asyncio
@pytest.mark.parametrize("src,dest", [tuple(shuffled(barebone_nodes())[:2]) for _ in range(PARAMETERIZED_SAMPLE_SIZE)])
async def test_hoprd_should_aggregate_and_redeem_tickets_in_channel_on_api_request(
    src: str, dest: str, swarm7: dict[str, Node]
):
    message_count = 2

    async with create_channel(swarm7[src], swarm7[dest], funding=message_count * TICKET_PRICE_PER_HOP) as channel:
        packets = [f"Channel agg and redeem on 1-hop: {src} - {dest} - {src} #{i:08d}" for i in range(message_count)]
        await send_and_receive_packets_with_pop(packets, src=swarm7[src], dest=swarm7[src], path=[swarm7[dest].peer_id])

        await asyncio.wait_for(check_unredeemed_tickets_value(swarm7[dest], message_count * TICKET_PRICE_PER_HOP), 30.0)

        ticket_statistics = await swarm7[dest].api.get_tickets_statistics()
        assert balance_str_to_int(ticket_statistics.unredeemed_value) == 2 * TICKET_PRICE_PER_HOP

        await asyncio.wait_for(swarm7[dest].api.channels_aggregate_tickets(channel), 20.0)

        ticket_statistics = await swarm7[dest].api.get_tickets_statistics()
        assert balance_str_to_int(ticket_statistics.unredeemed_value) == 2 * TICKET_PRICE_PER_HOP

        assert await swarm7[dest].api.channel_redeem_tickets(channel)

        await asyncio.wait_for(check_all_tickets_redeemed(swarm7[dest]), 120.0)

        ticket_statistics = await swarm7[dest].api.get_tickets_statistics()
        assert balance_str_to_int(ticket_statistics.unredeemed_value) == 0


@pytest.mark.asyncio
@pytest.mark.parametrize(
    "route",
    [shuffled(barebone_nodes())[:3] for _ in range(PARAMETERIZED_SAMPLE_SIZE)],
    # + [shuffled(nodes())[:5] for _ in range(PARAMETERIZED_SAMPLE_SIZE)],
)
async def test_hoprd_should_create_redeemable_tickets_on_routing_in_general_n_hop(route, swarm7: dict[str, Node]):
    message_count = int(TICKET_AGGREGATION_THRESHOLD / 10)

    async with AsyncExitStack() as channels:
        await asyncio.gather(
            *[
                channels.enter_async_context(
                    create_channel(swarm7[route[i]], swarm7[route[i + 1]], funding=message_count * TICKET_PRICE_PER_HOP)
                )
                for i in range(len(route) - 1)
            ]
        )

        packets = [f"General n-hop over {route} message #{i:08d}" for i in range(message_count)]
        await send_and_receive_packets_with_pop(
            packets,
            src=swarm7[route[0]],
            dest=swarm7[route[-1]],
            path=[swarm7[x].peer_id for x in route[1:-1]],
        )

        await asyncio.wait_for(
            check_unredeemed_tickets_value(swarm7[route[1]], message_count * TICKET_PRICE_PER_HOP), 30.0
        )

        await asyncio.sleep(10)  # wait for aggregation to finish before redeeming
        assert await swarm7[route[1]].api.tickets_redeem()

        await asyncio.wait_for(check_all_tickets_redeemed(swarm7[route[1]]), 120.0)


@pytest.mark.asyncio
@pytest.mark.parametrize("route", [shuffled(barebone_nodes())[:3] for _ in range(PARAMETERIZED_SAMPLE_SIZE)])
async def test_hoprd_should_be_able_to_close_open_channels_with_unredeemed_tickets(route, swarm7: dict[str, Node]):
    ticket_count = 2

    async with AsyncExitStack() as channels:
        await asyncio.gather(
            *[
                channels.enter_async_context(
                    create_channel(swarm7[route[i]], swarm7[route[i + 1]], funding=ticket_count * TICKET_PRICE_PER_HOP)
                )
                for i in range(len(route) - 1)
            ]
        )

        packets = [f"Channel unredeemed check: #{i:08d}" for i in range(ticket_count)]
        await send_and_receive_packets_with_pop(
            packets, src=swarm7[route[0]], dest=swarm7[route[-1]], path=[swarm7[route[1]].peer_id]
        )

        await asyncio.wait_for(
            check_unredeemed_tickets_value(swarm7[route[1]], ticket_count * TICKET_PRICE_PER_HOP), 30.0
        )

        # NOTE: will be closed on context manager exit


@pytest.mark.asyncio
@pytest.mark.parametrize("src,dest", random_distinct_pairs_from(barebone_nodes(), count=PARAMETERIZED_SAMPLE_SIZE))
async def test_hoprd_should_be_able_to_open_and_close_channel_without_tickets(
    src: str, dest: str, swarm7: dict[str, Node]
):
    async with create_channel(swarm7[src], swarm7[dest], OPEN_CHANNEL_FUNDING_VALUE_HOPR):
        # the context manager handles opening and closing of the channel with verification
        assert True


@pytest.mark.asyncio
@pytest.mark.parametrize(
    "route",
    [
        [
            random.sample(barebone_nodes(), 1)[0],
            random.sample(default_nodes(), 1)[0],
            random.sample(barebone_nodes(), 1)[0],
        ]
        for _ in range(PARAMETERIZED_SAMPLE_SIZE)
    ],
)
async def test_hoprd_default_strategy_automatic_ticket_aggregation_and_redeeming(route, swarm7: dict[str, Node]):
    ticket_count = int(TICKET_AGGREGATION_THRESHOLD)

    async with AsyncExitStack() as channels:
        await asyncio.gather(
            *[
                channels.enter_async_context(
                    create_channel(swarm7[route[i]], swarm7[route[i + 1]], funding=ticket_count * TICKET_PRICE_PER_HOP)
                )
                for i in range(len(route) - 1)
            ]
        )

        statistics_before = await swarm7[route[1]].api.get_tickets_statistics()

        packets = [f"Ticket aggregation test: #{i:08d}" for i in range(ticket_count)]
        await send_and_receive_packets_with_pop(
            packets, src=swarm7[route[0]], dest=swarm7[route[-1]], path=[swarm7[route[1]].peer_id]
        )

        async def aggregate_and_redeem_tickets(api: HoprdAPI):
            while True:
                statistics_after = await api.get_tickets_statistics()
                redeemed_value = balance_str_to_int(statistics_after.redeemed_value) - balance_str_to_int(
                    statistics_before.redeemed_value
                )

                if redeemed_value >= AGGREGATED_TICKET_PRICE:
                    break
                else:
                    await asyncio.sleep(0.1)

        await asyncio.wait_for(aggregate_and_redeem_tickets(swarm7[route[1]].api), 60.0)


# FIXME: This test depends on side-effects and cannot be run on its own. It
# should be redesigned.
@pytest.mark.asyncio
async def test_hoprd_sanity_check_channel_status(swarm7: dict[str, Node]):
    """
    The bash integration-test.sh opens and closes channels that can be visible inside this test scope
    """
    alice_api = swarm7["1"].api

    open_channels = await alice_api.all_channels(include_closed=False)
    open_and_closed_channels = await alice_api.all_channels(include_closed=True)

    assert len(open_and_closed_channels.all) >= len(open_channels.all), "Open and closed channels should be present"

    statuses = [c.status for c in open_and_closed_channels.all]
    assert "Closed" in statuses or "PendingToClose" in statuses, "Closed channels should be present"


@pytest.mark.asyncio
async def test_hoprd_strategy_UNFINISHED():
    """
    ## NOTE: strategy testing will require separate setup so commented out for now until moved
    # test_strategy_setting() {
    #   local node_api="${1}"

    #   settings=$(get_settings ${node_api})
    #   strategy=$(echo ${settings} | jq -r .strategy)
    #   [[ "${strategy}" != "passive" ]] && { msg "Default strategy should be passive, got: ${strategy}"; exit 1; }

    #   channels_count_pre=$(get_all_channels ${node_api} false | jq '.incoming | length')

    #   set_setting ${node_api} "strategy" "promiscuous"

    #   log "Waiting 100 seconds for the node to make connections to other nodes"
    #   sleep 100

    #   channels_count_post=$(get_all_channels ${node_api} false | jq '.incoming | length')
    #   [[ "${channels_count_pre}" -ge "${channels_count_post}" ]] && { msg "Node didn't open any connections by \
    #    itself even when strategy was set to promiscuous: ${channels_count_pre} !>= ${channels_count_post}"; exit 1; }
    #   echo "Strategy setting successfull"
    # }

    # test_strategy_setting ${api4}
    """
    assert True


@pytest.mark.asyncio
@pytest.mark.parametrize("peer", random.sample(barebone_nodes(), 1))
async def test_hoprd_check_native_withdraw(peer, swarm7: dict[str, Node]):
    amount = "9876"

    before_balance = await swarm7[peer].api.balances()
    await swarm7[peer].api.withdraw(amount, swarm7[peer].safe_address, "Native")
    after_balance = await swarm7[peer].api.balances()

    assert int(after_balance.safe_native) - int(before_balance.safe_native) == int(amount)


@pytest.mark.asyncio
@pytest.mark.parametrize("peer", random.sample(barebone_nodes(), 1))
async def test_hoprd_check_ticket_price_is_default(peer, swarm7: dict[str, Node]):
    price = await swarm7[peer].api.ticket_price()

    assert isinstance(price, int)
    assert price > 0


@pytest.mark.asyncio
@pytest.mark.parametrize("tag", [random.randint(0, RESERVED_TAG_UPPER_BOUND) for _ in range(5)])
async def test_send_message_with_reserved_application_tag_should_fail(tag: int, swarm7: dict[str, Node]):
    src, dest = random_distinct_pairs_from(barebone_nodes(), count=1)[0]

    await swarm7[src].api.send_message(
        swarm7[dest].peer_id, "This message should fail due to reserved tag", [], tag
    ) is None


@pytest.mark.asyncio
@pytest.mark.parametrize("tag", [random.randint(0, RESERVED_TAG_UPPER_BOUND) for _ in range(5)])
async def test_inbox_operations_with_reserved_application_tag_should_fail(tag: int, swarm7: dict[str, Node]):
    id = random.choice(barebone_nodes())

    await swarm7[id].api.messages_pop(tag) is None
    await swarm7[id].api.messages_peek(tag) is None
    await swarm7[id].api.messages_peek(tag) is None


@pytest.mark.asyncio
@pytest.mark.parametrize("src,dest", random_distinct_pairs_from(barebone_nodes(), count=PARAMETERIZED_SAMPLE_SIZE))
async def test_peeking_messages_with_timestamp(src: str, dest: str, swarm7: dict[str, Node]):
    message_count = int(TICKET_AGGREGATION_THRESHOLD / 10)
    split_index = int(message_count * 0.66)

    random_tag = random.randint(APPLICATION_TAG_THRESHOLD_FOR_SESSIONS, 65530)

    src_peer = swarm7[src]
    dest_peer = swarm7[dest]

    packets = [f"0 hop message #{i:08d}" for i in range(message_count)]
    for packet in packets[:split_index]:
        await src_peer.api.send_message(dest_peer.peer_id, packet, [], random_tag)

    await asyncio.sleep(2)

    for packet in packets[split_index:]:
        await src_peer.api.send_message(dest_peer.peer_id, packet, [], random_tag)

    await asyncio.wait_for(
        check_received_packets_with_peek(dest_peer, packets, tag=random_tag, sort=True), MULTIHOP_MESSAGE_SEND_TIMEOUT
    )

    packets = await dest_peer.api.messages_peek_all(random_tag)
    timestamps = sorted([message.received_at for message in packets.messages])

    # ts_for_query set right before (1ms before) the first message of the second batch.
    # This is to ensure that the first message of the second batch will be returned by the query.
    # It's a workaround, it should work properly without the -1, however randmly fails.
    ts_for_query = timestamps[split_index] - 1

    async def peek_the_messages():
        packets = await dest_peer.api.messages_peek_all(random_tag, ts_for_query)

        assert len(packets.messages) == message_count - split_index

    await asyncio.wait_for(peek_the_messages(), MULTIHOP_MESSAGE_SEND_TIMEOUT)


@pytest.mark.asyncio
@pytest.mark.parametrize("src,dest", random_distinct_pairs_from(barebone_nodes(), count=PARAMETERIZED_SAMPLE_SIZE))
async def test_send_message_return_timestamp(src: str, dest: str, swarm7: dict[str, Node]):
    message_count = int(TICKET_AGGREGATION_THRESHOLD / 10)
    random_tag = random.randint(APPLICATION_TAG_THRESHOLD_FOR_SESSIONS, 65530)

    src_peer = swarm7[src]
    dest_peer = swarm7[dest]

    packets = [f"0 hop message #{i:08d}" for i in range(message_count)]
    timestamps = []
    for packet in packets:
        res = await src_peer.api.send_message(dest_peer.peer_id, packet, [], random_tag)
        timestamps.append(res.timestamp)

    assert len(timestamps) == message_count
    assert timestamps == sorted(timestamps)


SERVER_LISTENING_PORT_HARDCODED_IN_HOPRD_CODE = 4677
HOPR_SESSION_MAX_PAYLOAD_SIZE = 462


def run_echo_server(port: int):

    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(('127.0.0.1', port))
        s.listen()
        conn, _addr = s.accept()
        with conn:
            while True:
                data = conn.recv(HOPR_SESSION_MAX_PAYLOAD_SIZE)
                conn.sendall(data)
                

@contextmanager
def echo_server(port: int):
    process = multiprocessing.Process(target=run_echo_server, args=(port,))
    process.start()
    try:
        yield port
    finally:
        process.terminate()
        

@contextmanager
def connect_socket(port):
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.connect(('127.0.0.1', port))
    
    try:
        yield s
    finally:
        s.close()


@pytest.mark.asyncio
@pytest.mark.parametrize("src,dest", random_distinct_pairs_from(barebone_nodes(), count=PARAMETERIZED_SAMPLE_SIZE))
async def test_session_communication_with_an_echo_server_wireguard_style_communication(src: str, dest: str, swarm7: dict[str, Node]):
    """
    HOPR TCP socket buffers are set to 462 bytes to mimic the underlying MTU of the HOPR protocol.
    """

    packet_count = 1000 if os.getenv("CI", default="false") == "false" else 50
    expected = [f"{i}".rjust(HOPR_SESSION_MAX_PAYLOAD_SIZE) for i in range(packet_count)]
    
    assert [len(x) for x in expected] == packet_count * [HOPR_SESSION_MAX_PAYLOAD_SIZE]

    src_peer = swarm7[src]
    dest_peer = swarm7[dest]

    # src_sock_port = await src_peer.api.session_client(dest_peer.peer_id, path={"Hops": 0})        # https://github.com/hoprnet/hoprnet/issues/6411
    src_sock_port = await src_peer.api.session_client(dest_peer.peer_id, path={"IntermediatePath": []})
    
    actual = []

    with echo_server(SERVER_LISTENING_PORT_HARDCODED_IN_HOPRD_CODE):
        # socket.listen does not actually listen immediately and needs some time to be working
        # otherwise a `ConnectionRefusedError: [Errno 61] Connection refused` will be encountered
        await asyncio.sleep(1.0)

        with connect_socket(src_sock_port) as s:
            s.settimeout(20)
            for message in expected:
                s.send(message.encode())

            for message in expected:
                actual.append(s.recv(len(message)).decode())

    actual.sort()
    expected.sort()
    assert actual == expected