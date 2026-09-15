#!/usr/bin/env python3
"""S0.3b — M-S1-7 MCP call round-trip through the candidate's OFFICIAL MCP SDK (E2), against the
local reference `echo` server (mcp_echo_server.py). Measures:
  - session setup: initialize + list_tools (the MCP handshake)
  - call round-trip: median of N `echo` tool calls (l1-spike-spec M-S1-7: "median of 20")
No network egress, no model calls. Emits `key value` lines. CC4: ecosystem named only in the ADR.
"""
import asyncio
import os
import sys
import time

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client

HERE = os.path.dirname(os.path.abspath(__file__))
N_CALLS = 20


async def run():
    params = StdioServerParameters(
        command=sys.executable,
        args=[os.path.join(HERE, "mcp_echo_server.py")],
    )
    t_connect = time.perf_counter()
    async with stdio_client(params) as (read, write):
        async with ClientSession(read, write) as session:
            t0 = time.perf_counter()
            await session.initialize()
            tools = await session.list_tools()
            setup_ms = (time.perf_counter() - t0) * 1000.0
            assert any(t.name == "echo" for t in tools.tools), "echo tool missing"

            # warm one call, then measure N round-trips
            await session.call_tool("echo", {"text": "warm"})
            samples = []
            for i in range(N_CALLS):
                s = time.perf_counter()
                res = await session.call_tool("echo", {"text": "ping-%d" % i})
                samples.append((time.perf_counter() - s) * 1000.0)
                got = res.content[0].text if res.content else None
                assert got == "ping-%d" % i, "echo mismatch: %r" % got
    total_ms = (time.perf_counter() - t_connect) * 1000.0
    samples.sort()
    median = samples[len(samples) // 2]
    p95 = samples[int(len(samples) * 0.95)]
    print("candidate E2")
    print("m_s1_7_transport mcp_stdio_official_sdk")
    print("m_s1_7_mcp_setup_ms %.3f" % setup_ms)
    print("m_s1_7_mcp_call_median_ms %.3f" % median)
    print("m_s1_7_mcp_call_p95_ms %.3f" % p95)
    print("m_s1_7_mcp_calls %d" % N_CALLS)
    print("m_s1_7_mcp_connect_total_ms %.3f" % total_ms)
    print("m_s1_7_mcp_echo_verified true")


if __name__ == "__main__":
    asyncio.run(run())
