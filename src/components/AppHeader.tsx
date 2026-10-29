import AppBar from "@mui/material/AppBar";
import Stack from "@mui/material/Stack";
import Toolbar from "@mui/material/Toolbar";
import Typography from "@mui/material/Typography";
import Chip from "@mui/material/Chip";
import type { Run } from "../App";
import { copy } from "../copy";
import type { Account } from "../lib/types";
import { AccountMenu } from "./AccountMenu";

/**
 * The title, and while signed in the Connected chip and the account menu. The signed-out,
 * signing-in and expired screens show the title alone.
 */
export function AppHeader({ signedIn, account, run }: { signedIn: boolean; account: Account | null; run: Run }) {
  return (
    <AppBar position="static">
      <Toolbar sx={{ justifyContent: "space-between", gap: 1, pl: 3, pr: signedIn ? 1 : 3 }}>
        <Typography variant="h6" component="h1">
          {copy.appName}
        </Typography>
        {signedIn && (
          <Stack direction="row" spacing={0.5} sx={{ alignItems: "center" }}>
            <Chip label={copy.header.connected} color="success" size="small" />
            <AccountMenu account={account} run={run} />
          </Stack>
        )}
      </Toolbar>
    </AppBar>
  );
}
