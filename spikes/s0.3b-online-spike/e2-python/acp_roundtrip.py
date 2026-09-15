#!/usr/bin/env python3
"""S0.3b — M-S1-7 ACP session setup through the candidate's OFFICIAL ACP SDK (E2), against the
local reference agent (acp_reference_agent.py). Drives the l1-spike-spec s3.1 item-5 sequence:
`initialize -> session/new -> session/prompt -> cancel -> close`, timing session setup
(initialize + new_session) as the M-S1-7 ACP figure. No network egress, no model calls."""
import asyncio
import os
import sys
import time

import acp
import acp.schema

HERE = os.path.dirname(os.path.abspath(__file__))


class ReferenceClient(acp.Client):
    """Minimal concrete client: the reference agent never calls back (no permission/elicitation)."""


async def run():
    proc = await asyncio.create_subprocess_exec(
        sys.executable,
        os.path.join(HERE, "acp_reference_agent.py"),
        stdin=asyncio.subprocess.PIPE,
        stdout=asyncio.subprocess.PIPE,
    )
    client = ReferenceClient()
    # connect_to_agent(client, input_stream=writer-to-agent, output_stream=reader-from-agent)
    conn = acp.connect_to_agent(client, proc.stdin, proc.stdout)

    t_setup = time.perf_counter()
    init = await conn.initialize(protocol_version=1)
    new = await conn.new_session(cwd=HERE)
    setup_ms = (time.perf_counter() - t_setup) * 1000.0
    session_id = new.session_id

    t_prompt = time.perf_counter()
    resp = await conn.prompt(
        session_id=session_id,
        prompt=[acp.schema.TextContentBlock(type="text", text="ping")],
    )
    prompt_ms = (time.perf_counter() - t_prompt) * 1000.0

    cancel_ok = False
    close_ok = False
    try:
        await conn.cancel(session_id=session_id)
        cancel_ok = True
    except Exception as e:
        print("acp_cancel_note %s" % type(e).__name__, file=sys.stderr)
    try:
        await conn.close_session(session_id=session_id)
        close_ok = True
    except Exception as e:
        print("acp_close_note %s" % type(e).__name__, file=sys.stderr)
    try:
        proc.stdin.close()
    except Exception:
        pass
    try:
        await asyncio.wait_for(proc.wait(), timeout=2.0)
    except Exception:
        proc.kill()

    print("candidate E2")
    print("m_s1_7_acp_transport acp_stdio_official_sdk")
    print("m_s1_7_acp_setup_ms %.3f" % setup_ms)
    print("m_s1_7_acp_prompt_ms %.3f" % prompt_ms)
    print("m_s1_7_acp_protocol_version %d" % init.protocol_version)
    print("m_s1_7_acp_stop_reason %s" % resp.stop_reason)
    print("m_s1_7_acp_cancel_ok %s" % ("true" if cancel_ok else "false"))
    print("m_s1_7_acp_close_ok %s" % ("true" if close_ok else "false"))
    print("m_s1_7_acp_session_verified %s" % ("true" if session_id and resp.stop_reason == "end_turn" else "false"))


if __name__ == "__main__":
    asyncio.run(run())
