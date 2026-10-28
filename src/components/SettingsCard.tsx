import Card from "@mui/material/Card";
import CardContent from "@mui/material/CardContent";
import Checkbox from "@mui/material/Checkbox";
import FormControlLabel from "@mui/material/FormControlLabel";
import Stack from "@mui/material/Stack";
import Typography from "@mui/material/Typography";
import type { Run } from "../App";
import { copy } from "../copy";
import { relay } from "../lib/relay";
import type { SettingsView } from "../lib/types";
import { CardHeading } from "./CardHeading";
import { PortField } from "./PortField";

export function SettingsCard({ settings, version, run }: { settings: SettingsView; version: string; run: Run }) {
  return (
    <Card variant="outlined" component="section" aria-labelledby="settings-heading">
      <CardContent>
        <Stack spacing={2}>
          <CardHeading id="settings-heading" title={copy.settings.heading} />
          <PortField key={settings.port} id="settings-port" port={settings.port} run={run} />
          <FormControlLabel
            control={
              <Checkbox
                checked={settings.autostart}
                onChange={(e) => {
                  const on = e.target.checked;
                  run(() => relay.setAutostart(on));
                }}
              />
            }
            label={copy.settings.autostart}
          />
          <Typography variant="caption" color="text.secondary">
            {copy.settings.version(version)}
          </Typography>
        </Stack>
      </CardContent>
    </Card>
  );
}
