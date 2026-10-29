import ArrowBackIcon from "@mui/icons-material/ArrowBack";
import Alert from "@mui/material/Alert";
import AppBar from "@mui/material/AppBar";
import Box from "@mui/material/Box";
import Card from "@mui/material/Card";
import CardContent from "@mui/material/CardContent";
import IconButton from "@mui/material/IconButton";
import Stack from "@mui/material/Stack";
import Tab from "@mui/material/Tab";
import Tabs from "@mui/material/Tabs";
import Toolbar from "@mui/material/Toolbar";
import Typography from "@mui/material/Typography";
import { useState } from "react";
import type { Run } from "../App";
import { CardHeading } from "../components/CardHeading";
import { CopyButton } from "../components/CopyButton";
import { StepNumber } from "../components/StepNumber";
import { TestSpotButton } from "../components/TestSpotButton";
import { copy } from "../copy";
import type { Account, ClusterView } from "../lib/types";

type Software = "n1mm" | "other";

/** One value in the Cluster address card, with its copy button. */
function AddressItem({ label, value, copyLabel }: { label: string; value: string; copyLabel: string }) {
  return (
    <Stack direction="row" spacing={1} sx={{ alignItems: "center" }}>
      <Box>
        <Typography variant="caption" color="text.secondary" component="span" sx={{ display: "block" }}>
          {label}
        </Typography>
        <Typography variant="subtitle1" component="span">
          {value}
        </Typography>
      </Box>
      <CopyButton text={value} label={copyLabel} size="medium" />
    </Stack>
  );
}

/**
 * "Set up logging software" from the DX cluster tab: where logging software connects and how,
 * for N1MM Logger+ or anything else, and a test spot. It replaces the tabs until Back. The LAN
 * address shows only while other computers may connect (plan decision 6).
 */
export function DxSetupScreen({
  cluster,
  account,
  run,
  onBack,
}: {
  cluster: ClusterView;
  account: Account | null;
  run: Run;
  onBack: () => void;
}) {
  const [software, setSoftware] = useState<Software>("n1mm");
  const lan = cluster.addresses.lan;
  const listening = cluster.enabled && cluster.listener.state === "listening";
  const steps = [
    ...copy.dxSetup.steps[software].map((title) => ({ title, text: null as string | null, test: false })),
    { title: copy.dxSetup.connect, text: lan ? copy.dxSetup.lanAddress(lan) : null, test: false },
    { title: copy.dxSetup.check, text: copy.dxSetup.checkText, test: true },
  ];
  return (
    <>
      <AppBar position="static">
        <Toolbar sx={{ gap: 1, pl: 1, pr: 2 }}>
          {/* The screen opens from a button; Back is where focus goes. */}
          <IconButton color="inherit" aria-label={copy.dxSetup.back} onClick={onBack} autoFocus>
            <ArrowBackIcon />
          </IconButton>
          <Typography variant="h6" component="h1">
            {copy.setUpLogging}
          </Typography>
        </Toolbar>
      </AppBar>
      <Stack component="main" spacing={2} sx={{ flexGrow: 1, p: 3 }}>
        <Typography variant="body2" color="text.secondary">
          {copy.dxSetup.intro}
        </Typography>

        <Box sx={{ borderBottom: 1, borderColor: "divider" }}>
          <Tabs value={software} onChange={(_, next: Software) => setSoftware(next)} aria-label={copy.dxSetup.software}>
            <Tab value="n1mm" label={copy.dxSetup.n1mm} id="setup-tab-n1mm" aria-controls="setup-panel" />
            <Tab value="other" label={copy.dxSetup.other} id="setup-tab-other" aria-controls="setup-panel" />
          </Tabs>
        </Box>

        <Stack spacing={2} id="setup-panel" role="tabpanel" aria-labelledby={`setup-tab-${software}`}>
          <Card variant="outlined" component="section" aria-labelledby="setup-address-heading">
            <CardContent>
              <Stack spacing={1.5}>
                <CardHeading id="setup-address-heading" title={copy.dxSetup.addressHeading} />
                <Stack direction="row" sx={{ flexWrap: "wrap", columnGap: 3, rowGap: 1 }}>
                  <AddressItem
                    label={copy.cluster.address}
                    value={cluster.addresses.local}
                    copyLabel={copy.cluster.copyAddress(cluster.addresses.local)}
                  />
                  <AddressItem
                    label={copy.cluster.port}
                    value={String(cluster.port)}
                    copyLabel={copy.cluster.copyPort(cluster.port)}
                  />
                  {account && (
                    <AddressItem
                      label={copy.dxSetup.callsign}
                      value={account.callsign}
                      copyLabel={copy.cluster.copyCallsign(account.callsign)}
                    />
                  )}
                  {lan && (
                    <AddressItem
                      label={copy.cluster.otherComputers}
                      value={lan}
                      copyLabel={copy.cluster.copyAddress(lan)}
                    />
                  )}
                </Stack>
              </Stack>
            </CardContent>
          </Card>

          <Card variant="outlined" component="section" aria-labelledby="setup-steps-heading">
            <CardContent>
              <Stack spacing={1.5}>
                <CardHeading id="setup-steps-heading" title={copy.dxSetup.stepsHeading} />
                <Stack component="ol" spacing={1.5} sx={{ m: 0, p: 0, listStyle: "none" }}>
                  {steps.map((step, i) => (
                    <Stack
                      component="li"
                      key={step.title}
                      direction="row"
                      spacing={2}
                      sx={{ alignItems: "flex-start" }}
                    >
                      <StepNumber aria-hidden>{i + 1}</StepNumber>
                      <Stack spacing={1} sx={{ alignItems: "flex-start" }}>
                        <Box>
                          <Typography variant="body1">{step.title}</Typography>
                          {step.text && (
                            <Typography variant="body2" color="text.secondary">
                              {step.text}
                            </Typography>
                          )}
                        </Box>
                        {step.test && <TestSpotButton disabled={!listening} run={run} showResult />}
                      </Stack>
                    </Stack>
                  ))}
                </Stack>
              </Stack>
            </CardContent>
          </Card>

          <Alert severity="info" role="status">
            {copy.dxSetup.filters[software]}
          </Alert>
        </Stack>
      </Stack>
    </>
  );
}
