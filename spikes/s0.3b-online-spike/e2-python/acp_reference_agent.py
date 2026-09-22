#!/usr/bin/env python3
"""S0.3b — local reference ACP agent (throwaway). The fixed local reference peer M-S1-7 sets up an
ACP stdio session against (l1-spike-spec s3.1 item 5), served through the candidate's OFFICIAL ACP
SDK. No model spend: `prompt` returns a fixed `end_turn` stop immediately."""
import asyncio

import acp


class ReferenceAgent(acp.Agent):
    async def initialize(self, protocol_version, client_capabilities=None, client_info=None, **kw):
        return acp.InitializeResponse(protocol_version=protocol_version, agent_capabilities=None)

    async def new_session(self, cwd, additional_directories=None, mcp_servers=None, **kw):
        return acp.NewSessionResponse(session_id="s03b-ref-1")

    async def prompt(self, session_id, prompt, **kw):
        return acp.PromptResponse(stop_reason="end_turn")

    async def cancel(self, session_id, **kw):
        return None

    async def close_session(self, session_id, **kw):
        return acp.CloseSessionResponse()


if __name__ == "__main__":
    asyncio.run(acp.run_agent(ReferenceAgent()))
