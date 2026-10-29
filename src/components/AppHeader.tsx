import AppBar from "@mui/material/AppBar";
import Stack from "@mui/material/Stack";
import Toolbar from "@mui/material/Toolbar";
import Typography from "@mui/material/Typography";
import Chip from "@mui/material/Chip";
import type { Run } from "../App";
import { copy } from "../copy";
import type { Account, UpstreamStatus } from "../lib/types";
import { upstreamChip } from "../lib/upstream";
import { AccountMenu } from "./AccountMenu";

/**
 * The title, and while signed in the connection chip and the account menu. The signed-out,
 * signing-in and expired screens show the title alone.
 */
export function AppHeader({
  signedIn,
  account,
  upstream,
  run,
}: {
  signedIn: boolean;
  account: Account | null;
  /** Only read while signed in. */
  upstream?: UpstreamStatus;
  run: Run;
}) {
  const chip = upstream ? upstreamChip(upstream) : null;
  return (
    <AppBar position="static">
      <Toolbar sx={{ justifyContent: "space-between", gap: 1, pl: 3, pr: signedIn ? 1 : 3 }}>
        <Typography variant="h6" component="h1">
          {copy.appName}
        </Typography>
        {signedIn && (
          <Stack direction="row" spacing={0.5} sx={{ alignItems: "center" }}>
            {chip && <Chip size="small" {...chip} />}
            <AccountMenu account={account} run={run} />
          </Stack>
        )}
      </Toolbar>
    </AppBar>
  );
}
