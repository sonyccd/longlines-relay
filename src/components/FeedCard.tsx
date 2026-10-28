import Alert from "@mui/material/Alert";
import AlertTitle from "@mui/material/AlertTitle";
import Button from "@mui/material/Button";
import Card from "@mui/material/Card";
import CardContent from "@mui/material/CardContent";
import Checkbox from "@mui/material/Checkbox";
import Chip, { type ChipProps } from "@mui/material/Chip";
import Divider from "@mui/material/Divider";
import FormControlLabel from "@mui/material/FormControlLabel";
import Paper from "@mui/material/Paper";
import Stack from "@mui/material/Stack";
import Typography from "@mui/material/Typography";
import OpenInNewIcon from "@mui/icons-material/OpenInNew";
import type { Run } from "../App";
import { copy } from "../copy";
import { relay } from "../lib/relay";
import { ago } from "../lib/time";
import type { Account, FeedStatus, FeedView } from "../lib/types";
import { useNow } from "../lib/useNow";
import { CardHeading } from "./CardHeading";
import { PortField } from "./PortField";

/** The chip's look per status, as the mockups draw it. */
const CHIP: Record<FeedStatus, Pick<ChipProps, "color" | "variant">> = {
  receiving: { color: "success", variant: "filled" },
  waiting: { color: "default", variant: "outlined" },
  ignoring_contacts: { color: "warning", variant: "filled" },
  port_in_use: { color: "error", variant: "filled" },
  test_mode: { color: "default", variant: "outlined" },
};

export function FeedCard({ feed, account, run }: { feed: FeedView; account: Account | null; run: Run }) {
  const now = useNow(1000);
  const portInUse = feed.listener.state === "port_in_use";
  const waiting = feed.status === "waiting";
  return (
    <Card variant="outlined" component="section" aria-labelledby="feed-heading">
      <CardContent>
        <Stack spacing={2}>
          <CardHeading
            id="feed-heading"
            title={copy.feed.heading}
            chip={<Chip label={feed.label} size="small" {...CHIP[feed.status]} />}
          />
          <Stack>
            <Typography variant="body2">
              {feed.last_packet_at
                ? copy.feed.lastContact(ago(new Date(feed.last_packet_at), now))
                : copy.feed.nothingYet}
            </Typography>
            {!portInUse && (
              <Typography variant="body2" color="text.secondary">
                {copy.feed.listening(feed.port)}
              </Typography>
            )}
          </Stack>

          {portInUse && (
            <Alert severity="error">
              <AlertTitle>{copy.feed.portInUse(feed.port)}</AlertTitle>
              <Stack spacing={2}>
                <span>{copy.feed.portInUseHelp}</span>
                <PortField key={feed.port} id="feed-port" port={feed.port} run={run} />
              </Stack>
            </Alert>
          )}

          {feed.mismatch && account && (
            <Alert severity="warning">
              <AlertTitle>{copy.feed.mismatchTitle(feed.mismatch)}</AlertTitle>
              {copy.feed.mismatchText(account.callsign, feed.mismatch)}
            </Alert>
          )}

          {feed.test_mode && (
            <Alert severity="info" role="status">
              {copy.feed.testModeOn}
            </Alert>
          )}

          {waiting && (
            <Paper elevation={1}>
              <Stack spacing={1.5} sx={{ p: 2, alignItems: "flex-start" }}>
                <Typography variant="body2">{copy.feed.waitingHelp(feed.port)}</Typography>
                <Button variant="contained" endIcon={<OpenInNewIcon />} onClick={() => run(relay.openSetupDocs)}>
                  {copy.setUpLogging}
                </Button>
              </Stack>
            </Paper>
          )}

          <Divider />
          <FormControlLabel
            sx={{ alignItems: "flex-start", mx: 0, gap: 1.5 }}
            control={
              <Checkbox
                checked={feed.test_mode}
                onChange={(e) => {
                  const on = e.target.checked;
                  run(() => relay.setTestMode(on));
                }}
                sx={{ p: 0, mt: 0.25 }}
                slotProps={{ input: { "aria-describedby": "test-mode-help" } }}
              />
            }
            label={
              <Stack>
                <Typography variant="body1">{copy.feed.testMode}</Typography>
                <Typography variant="body2" color="text.secondary" id="test-mode-help">
                  {copy.feed.testModeHelp}
                </Typography>
              </Stack>
            }
          />
        </Stack>
      </CardContent>
    </Card>
  );
}
