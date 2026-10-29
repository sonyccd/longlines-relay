import Alert from "@mui/material/Alert";
import AlertTitle from "@mui/material/AlertTitle";
import { copy } from "../copy";
import { countdown } from "../lib/time";
import type { UpstreamStatus } from "../lib/types";
import { useNow } from "../lib/useNow";

/** "in 0:42", or "shortly" once the time has passed. */
function when(at: string, now: Date): string {
  const until = new Date(at);
  return until > now ? copy.header.in(countdown(until, now)) : copy.header.soon;
}

/** Why contacts aren't going through, at the top of the Contacts tab. Nothing while connected. */
export function UpstreamAlert({ upstream }: { upstream: UpstreamStatus }) {
  const now = useNow(1000);
  switch (upstream.state) {
    case "retrying":
      return (
        <Alert severity="warning" role="status">
          <AlertTitle>{copy.header.retryingTitle}</AlertTitle>
          {copy.header.retryingText(upstream.reason, when(upstream.at, now))}
        </Alert>
      );
    case "rate_limited":
      return (
        <Alert severity="warning" role="status">
          <AlertTitle>{copy.header.rateLimitedTitle}</AlertTitle>
          {copy.header.rateLimitedText(when(upstream.until, now))}
        </Alert>
      );
    case "credential_store_unavailable":
      return (
        <Alert severity="error" role="status">
          <AlertTitle>{copy.header.credentialStoreTitle}</AlertTitle>
          {copy.header.credentialStoreText}
        </Alert>
      );
    case "connected":
    case "signed_out":
      return null;
  }
}
