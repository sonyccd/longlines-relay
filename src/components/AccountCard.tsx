import Button from "@mui/material/Button";
import Card from "@mui/material/Card";
import CardContent from "@mui/material/CardContent";
import Chip from "@mui/material/Chip";
import Skeleton from "@mui/material/Skeleton";
import Stack from "@mui/material/Stack";
import Typography from "@mui/material/Typography";
import type { Run } from "../App";
import { copy } from "../copy";
import { relay } from "../lib/relay";
import type { Account } from "../lib/types";
import { CardHeading } from "./CardHeading";

/** Long Lines account. `account` is null until `GET /me` answers (for example, offline at launch). */
export function AccountCard({ account, run }: { account: Account | null; run: Run }) {
  return (
    <Card variant="outlined" component="section" aria-labelledby="account-heading">
      <CardContent>
        <Stack spacing={2}>
          <CardHeading
            id="account-heading"
            title={copy.account.heading}
            chip={<Chip label={copy.account.connected} color="success" size="small" />}
          />
          <Stack direction="row" spacing={2} sx={{ alignItems: "center", justifyContent: "space-between" }}>
            <Stack sx={{ minWidth: 0 }}>
              <Typography variant="body2" color="text.secondary">
                {copy.account.signedInAs}
              </Typography>
              {account ? (
                <>
                  <Typography variant="subtitle2">{account.callsign}</Typography>
                  <Typography variant="body2" color="text.secondary" noWrap>
                    {account.email}
                  </Typography>
                </>
              ) : (
                <>
                  <Skeleton width={80} />
                  <Skeleton width={200} />
                </>
              )}
            </Stack>
            <Button variant="outlined" onClick={() => run(relay.signOut)}>
              {copy.account.signOut}
            </Button>
          </Stack>
          <Typography variant="body2" color="text.secondary">
            {copy.account.privacy}
          </Typography>
        </Stack>
      </CardContent>
    </Card>
  );
}
