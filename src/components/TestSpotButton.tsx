import Button from "@mui/material/Button";
import Stack from "@mui/material/Stack";
import Typography from "@mui/material/Typography";
import { useState } from "react";
import type { Run } from "../App";
import { copy } from "../copy";
import { relay } from "../lib/relay";

type Result = "sent" | "failed" | null;

/**
 * "Send test spot": one line to every program connected to the DX cluster. On the DX cluster
 * tab the row under "Spots sent" is the answer; the setup screen, which has no list, says
 * whether it went (`showResult`).
 */
export function TestSpotButton({ disabled, run, showResult }: { disabled: boolean; run: Run; showResult: boolean }) {
  const [result, setResult] = useState<Result>(null);
  const send = () => {
    setResult(null);
    run(async () => {
      try {
        const view = await relay.sendTestSpot();
        setResult("sent");
        return view;
      } catch (error) {
        setResult("failed");
        throw error;
      }
    });
  };
  const button = (
    <Button variant="outlined" disabled={disabled} onClick={send}>
      {copy.cluster.sendTest}
    </Button>
  );
  if (!showResult) return button;
  return (
    <Stack spacing={1} sx={{ alignItems: "flex-start" }}>
      {button}
      <Typography variant="caption" color={result === "failed" ? "error" : "text.secondary"} role="status">
        {result === "sent" ? copy.dxSetup.testSent : result === "failed" ? copy.dxSetup.testFailed : null}
      </Typography>
    </Stack>
  );
}
