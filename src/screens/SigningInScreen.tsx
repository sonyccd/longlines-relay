import Box from "@mui/material/Box";
import Button from "@mui/material/Button";
import LinearProgress from "@mui/material/LinearProgress";
import Link from "@mui/material/Link";
import Paper from "@mui/material/Paper";
import Stack from "@mui/material/Stack";
import Typography from "@mui/material/Typography";
import type { Run } from "../App";
import { copy } from "../copy";
import { relay } from "../lib/relay";
import { countdown, withoutScheme } from "../lib/time";
import type { PairingView } from "../lib/types";
import { useNow } from "../lib/useNow";

type Waiting = Extract<PairingView, { state: "waiting" }>;

/** Screen 2: the code, a countdown, and Cancel. */
export function SigningInScreen({ pairing, run }: { pairing: Waiting; run: Run }) {
  const now = useNow(1000);
  return (
    <Stack component="main" spacing={4} sx={{ px: 5, py: 6 }}>
      <Box>
        <Typography variant="h5" component="h2" gutterBottom>
          {copy.signingIn.heading}
        </Typography>
        <Typography variant="body1" color="text.secondary">
          {copy.signingIn.body}
        </Typography>
      </Box>

      <Paper variant="outlined">
        <Stack spacing={1} sx={{ p: 3, alignItems: "center" }}>
          <Typography variant="overline" color="text.secondary" component="span">
            {copy.signingIn.yourCode}
          </Typography>
          <Typography variant="code" component="span" data-testid="user-code">
            {pairing.user_code}
          </Typography>
          <Typography variant="body2" color="text.secondary">
            {copy.signingIn.expiresIn(countdown(new Date(pairing.expires_at), now))}
          </Typography>
        </Stack>
      </Paper>

      <Stack role="status" spacing={1}>
        <Typography variant="body2">{copy.signingIn.waiting}</Typography>
        <LinearProgress aria-label={copy.signingIn.waiting} />
      </Stack>

      <Stack direction="row" spacing={1} sx={{ alignItems: "center" }}>
        <Button variant="outlined" onClick={() => run(relay.cancelSignIn)}>
          {copy.signingIn.cancel}
        </Button>
        <Button onClick={() => run(relay.openSignInPage)}>{copy.signingIn.openAgain}</Button>
      </Stack>

      <Typography variant="body2" color="text.secondary">
        {copy.signingIn.browserDidntOpen}{" "}
        <Link component="button" variant="body2" onClick={() => run(relay.openSignInPage)}>
          {withoutScheme(pairing.verification_uri)}
        </Link>{" "}
        {copy.signingIn.enterTheCode}
      </Typography>
    </Stack>
  );
}
