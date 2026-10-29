import OpenInNewIcon from "@mui/icons-material/OpenInNew";
import Alert from "@mui/material/Alert";
import AlertTitle from "@mui/material/AlertTitle";
import Box from "@mui/material/Box";
import Button from "@mui/material/Button";
import Card from "@mui/material/Card";
import CardContent from "@mui/material/CardContent";
import Checkbox from "@mui/material/Checkbox";
import Chip, { type ChipProps } from "@mui/material/Chip";
import FormControlLabel from "@mui/material/FormControlLabel";
import Stack from "@mui/material/Stack";
import Switch from "@mui/material/Switch";
import Typography from "@mui/material/Typography";
import { Fragment, useState } from "react";
import type { Run } from "../App";
import { copy } from "../copy";
import { noDestinationYet } from "../lib/cluster";
import { relay } from "../lib/relay";
import { ago, countdown } from "../lib/time";
import type { ClusterStatus, ClusterView, SentRow, SettingsView } from "../lib/types";
import { useNow } from "../lib/useNow";
import { CopyButton } from "./CopyButton";
import { PortField } from "./PortField";
import { TestSpotButton } from "./TestSpotButton";

/** The chip's look per status, as the mockups draw it (Off and Listening outlined, Running green). */
const CHIP: Record<ClusterStatus, Pick<ChipProps, "color" | "variant">> = {
  off: { color: "default", variant: "outlined" },
  listening: { color: "default", variant: "outlined" },
  running: { color: "success", variant: "filled" },
  port_in_use: { color: "error", variant: "filled" },
  scope_missing: { color: "warning", variant: "filled" },
};

/** The address and port rows, with copy buttons. The network row shows only while it is on. */
function Addresses({ cluster }: { cluster: ClusterView }) {
  const rows: { label: string; address: string }[] = [
    { label: copy.cluster.thisComputer, address: cluster.addresses.local },
  ];
  if (cluster.addresses.lan) rows.push({ label: copy.cluster.otherComputers, address: cluster.addresses.lan });
  const cell = (text: string, label: string) => (
    <Stack direction="row" spacing={0.5} sx={{ alignItems: "center", minWidth: 0 }}>
      <Typography variant="subtitle1" component="span" noWrap>
        {text}
      </Typography>
      <CopyButton text={text} label={label} size="small" />
    </Stack>
  );
  return (
    <Box
      role="group"
      aria-label={copy.cluster.details}
      sx={{
        display: "grid",
        gridTemplateColumns: "130px minmax(0, 1fr) minmax(0, 1fr)",
        columnGap: 1,
        alignItems: "center",
      }}
    >
      <span />
      <Typography variant="caption" color="text.secondary">
        {copy.cluster.address}
      </Typography>
      <Typography variant="caption" color="text.secondary">
        {copy.cluster.port}
      </Typography>
      {rows.map((row) => (
        <Fragment key={row.label}>
          <Typography variant="body2" color="text.secondary">
            {row.label}
          </Typography>
          {cell(row.address, copy.cluster.copyAddress(row.address))}
          {cell(String(cluster.port), copy.cluster.copyPort(cluster.port))}
        </Fragment>
      ))}
    </Box>
  );
}

/**
 * The DX cluster tab's card: the switch and chip, what has been sent, where logging software
 * connects, the network setting and the buttons. Off, it says what the cluster is and that
 * nothing listens.
 */
export function DxClusterCard({
  cluster,
  sent,
  settings,
  run,
  onSetUp,
}: {
  cluster: ClusterView;
  sent: SentRow[];
  settings: SettingsView;
  run: Run;
  onSetUp: () => void;
}) {
  const now = useNow(1000);
  const [changingPort, setChangingPort] = useState(false);
  const listening = cluster.listener.state === "listening";
  const portInUse = cluster.status === "port_in_use";
  const portMoved = listening && cluster.port !== settings.cluster_port;
  const noDestination = noDestinationYet(cluster, sent);
  const retrying =
    cluster.upstream.state === "retrying" && cluster.status !== "scope_missing" ? cluster.upstream : null;

  return (
    <Card variant="outlined" component="section" aria-label={copy.cluster.heading}>
      <CardContent>
        <Stack spacing={1.5}>
          <Stack direction="row" spacing={1} sx={{ alignItems: "center", justifyContent: "space-between" }}>
            <FormControlLabel
              sx={{ mr: 0 }}
              control={
                <Switch
                  checked={cluster.enabled}
                  onChange={(e) => {
                    const on = e.target.checked;
                    setChangingPort(false);
                    run(() => relay.setClusterEnabled(on));
                  }}
                />
              }
              label={
                <Typography variant="subtitle1" component="span">
                  {copy.cluster.heading}
                </Typography>
              }
            />
            <Chip label={cluster.label} size="small" {...CHIP[cluster.status]} />
          </Stack>

          {!cluster.enabled ? (
            <>
              <Typography variant="body2" color="text.secondary">
                {copy.cluster.offText}
              </Typography>
              <Typography variant="caption" color="text.secondary">
                {copy.cluster.offNote}
              </Typography>
            </>
          ) : (
            <>
              <Box>
                <Typography variant="body2">
                  {cluster.last_sent_at
                    ? copy.cluster.lastSpot(ago(new Date(cluster.last_sent_at), now), cluster.sent_today)
                    : copy.cluster.noSpots}
                </Typography>
                <Typography variant="caption" color="text.secondary">
                  {copy.cluster.programs(cluster.clients)}
                </Typography>
              </Box>

              {cluster.status === "scope_missing" && (
                <Alert
                  severity="warning"
                  action={
                    <Button
                      color="inherit"
                      size="small"
                      onClick={() =>
                        run(async () => {
                          await relay.signOut();
                          return relay.startSignIn();
                        })
                      }
                    >
                      {copy.cluster.signInAgain}
                    </Button>
                  }
                >
                  <AlertTitle>{copy.cluster.scopeMissingTitle}</AlertTitle>
                  {copy.cluster.scopeMissingText}
                </Alert>
              )}

              {portInUse && (
                <Alert severity="error">
                  <AlertTitle>{copy.feed.portInUse(cluster.port)}</AlertTitle>
                  <Stack spacing={2}>
                    <span>{copy.feed.portInUseHelp}</span>
                    <PortField
                      key={settings.cluster_port}
                      id="cluster-port"
                      label={copy.cluster.port}
                      port={settings.cluster_port}
                      onChange={(port) => run(() => relay.setClusterPort(port))}
                    />
                  </Stack>
                </Alert>
              )}

              {retrying && (
                <Alert severity="warning" role="status">
                  {copy.cluster.retrying(countdown(new Date(retrying.at), now))}
                </Alert>
              )}

              {noDestination && (
                <Alert severity="info" role="status">
                  {copy.cluster.noDestination}
                </Alert>
              )}

              {portMoved && (
                <Alert severity="info">{copy.cluster.portMoved(settings.cluster_port, cluster.port)}</Alert>
              )}

              {listening && <Addresses cluster={cluster} />}

              <Box>
                <FormControlLabel
                  control={
                    <Checkbox
                      checked={cluster.lan}
                      onChange={(e) => {
                        const on = e.target.checked;
                        run(() => relay.setClusterLan(on));
                      }}
                      slotProps={{ input: { "aria-describedby": "cluster-lan-note" } }}
                    />
                  }
                  label={copy.cluster.lan}
                />
                <Typography variant="caption" color="text.secondary" component="p" id="cluster-lan-note" sx={{ pl: 4 }}>
                  {copy.cluster.lanNote}
                </Typography>
              </Box>

              <Stack direction="row" sx={{ flexWrap: "wrap", gap: 1, alignItems: "center" }}>
                {noDestination && (
                  <Button variant="outlined" endIcon={<OpenInNewIcon />} onClick={() => run(relay.openAddDestination)}>
                    {copy.cluster.addDestination}
                  </Button>
                )}
                <Button variant="outlined" onClick={onSetUp}>
                  {copy.setUpLogging}
                </Button>
                <TestSpotButton disabled={!listening} run={run} showResult={false} />
                {!portInUse && (
                  <Button
                    aria-expanded={changingPort}
                    aria-controls={changingPort ? "cluster-port-form" : undefined}
                    onClick={() => setChangingPort((open) => !open)}
                  >
                    {copy.cluster.changePort}
                  </Button>
                )}
              </Stack>

              {changingPort && !portInUse && (
                <Box id="cluster-port-form">
                  <PortField
                    key={settings.cluster_port}
                    id="cluster-port"
                    label={copy.cluster.port}
                    port={settings.cluster_port}
                    onChange={(port) => {
                      setChangingPort(false);
                      run(() => relay.setClusterPort(port));
                    }}
                  />
                </Box>
              )}
            </>
          )}
        </Stack>
      </CardContent>
    </Card>
  );
}
