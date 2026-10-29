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
import { PortField } from "./PortField";

/** The chip's look per status, as the mockups draw it. */
const CHIP: Record<FeedStatus, Pick<ChipProps, "color" | "variant">> = {
  receiving: { color: "success", variant: "filled" },
  waiting: { color: "default", variant: "outlined" },
  ignoring_contacts: { color: "warning", variant: "filled" },
  port_in_use: { color: "error", variant: "filled" },
  test_mode: { color: "default", variant: "outlined" },
};

/**
 * The logging feed, at the top of the Contacts tab: the chip, the last contact, the port, Test
 * mode, and whatever needs attention. The help line and the "Set up logging software" link show
 * only while nothing else does, as the RelayTab* mockups draw it.
 */
export function FeedCard({ feed, account, run }: { feed: FeedView; account: Account | null; run: Run }) {
  const now = useNow(1000);
  const portInUse = feed.listener.state === "port_in_use";
  const waiting = feed.status === "waiting";
  const mismatch = feed.mismatch !== null && account !== null;
  const quiet = !portInUse && !mismatch && !feed.test_mode && !waiting;
  const helpShown = quiet || (waiting && !feed.test_mode);
  const statusLine = [
    feed.last_packet_at ? null : copy.feed.nothingYet,
    portInUse ? null : copy.feed.listening(feed.port),
  ]
    .filter((part) => part !== null)
    .join(" · ");
  const help = (
    <Typography variant="caption" color="text.secondary" id="test-mode-help">
      {copy.feed.testModeHelp}
    </Typography>
  );
  return (
    <Card variant="outlined" component="section" aria-label={copy.feed.heading}>
      <CardContent>
        <Stack spacing={1.5}>
          <Stack direction="row" spacing={2} sx={{ alignItems: "flex-start", justifyContent: "space-between" }}>
            <Stack spacing={0.5}>
              <Stack direction="row" spacing={1} sx={{ alignItems: "center", flexWrap: "wrap" }}>
                <Chip label={feed.label} size="small" {...CHIP[feed.status]} />
                {feed.last_packet_at && (
                  <Typography variant="body2">
                    {copy.feed.lastContact(ago(new Date(feed.last_packet_at), now))}
                  </Typography>
                )}
              </Stack>
              {statusLine && (
                <Typography variant="caption" color="text.secondary">
                  {statusLine}
                </Typography>
              )}
            </Stack>
            <FormControlLabel
              sx={{ flex: "none", mr: 0 }}
              control={
                <Checkbox
                  size="small"
                  checked={feed.test_mode}
                  onChange={(e) => {
                    const on = e.target.checked;
                    run(() => relay.setTestMode(on));
                  }}
                  slotProps={{
                    input: { "aria-describedby": helpShown ? "test-mode-help" : undefined },
                  }}
                />
              }
              label={<Typography variant="body2">{copy.feed.testMode}</Typography>}
            />
          </Stack>

          {portInUse && (
            <Alert severity="error">
              <AlertTitle>{copy.feed.portInUse(feed.port)}</AlertTitle>
              <Stack spacing={2}>
                <span>{copy.feed.portInUseRetryHelp}</span>
                <PortField
                  key={feed.port}
                  id="feed-port"
                  label={copy.settings.port}
                  port={feed.port}
                  onChange={(port) => run(() => relay.setPort(port))}
                />
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
            <>
              <Paper elevation={1}>
                <Stack spacing={1.5} sx={{ p: 2, alignItems: "flex-start" }}>
                  <Typography variant="body2">{copy.feed.waitingHelp(feed.port)}</Typography>
                  <Button variant="contained" endIcon={<OpenInNewIcon />} onClick={() => run(relay.openSetupDocs)}>
                    {copy.setUpLogging}
                  </Button>
                </Stack>
              </Paper>
              {helpShown && help}
            </>
          )}

          {quiet && (
            <>
              <Divider />
              <Stack
                direction="row"
                sx={{
                  alignItems: "center",
                  justifyContent: "space-between",
                  flexWrap: "wrap",
                  columnGap: 2,
                  rowGap: 0.5,
                }}
              >
                {help}
                <Button size="small" endIcon={<OpenInNewIcon />} onClick={() => run(relay.openSetupDocs)}>
                  {copy.setUpLogging}
                </Button>
              </Stack>
            </>
          )}
        </Stack>
      </CardContent>
    </Card>
  );
}
