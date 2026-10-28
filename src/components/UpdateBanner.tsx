import Alert from "@mui/material/Alert";
import Button from "@mui/material/Button";
import type { Run } from "../App";
import { copy } from "../copy";
import { relay } from "../lib/relay";
import type { UpdateView } from "../lib/types";

/** Offers a downloaded-on-request update. Nothing shows while the relay is up to date. */
export function UpdateBanner({ update, run }: { update: UpdateView; run: Run }) {
  switch (update.state) {
    case "none":
      return null;
    case "available":
      return (
        <Alert
          severity="info"
          action={
            <Button color="inherit" size="small" onClick={() => run(relay.installUpdate)}>
              {copy.update.install}
            </Button>
          }
        >
          {copy.update.available(update.version)}
        </Alert>
      );
    case "installing":
      return (
        <Alert severity="info" role="status">
          {copy.update.installing(update.version)}
        </Alert>
      );
    case "failed":
      return (
        <Alert
          severity="error"
          action={
            <Button color="inherit" size="small" onClick={() => run(relay.installUpdate)}>
              {copy.update.retry}
            </Button>
          }
        >
          {copy.update.failed(update.version)}
        </Alert>
      );
  }
}
