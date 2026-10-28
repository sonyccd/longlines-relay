import Alert from "@mui/material/Alert";
import AlertTitle from "@mui/material/AlertTitle";
import Box from "@mui/material/Box";
import Button from "@mui/material/Button";
import Link from "@mui/material/Link";
import Stack from "@mui/material/Stack";
import Typography from "@mui/material/Typography";
import type { Run } from "../App";
import { StepNumber } from "../components/StepNumber";
import { copy } from "../copy";
import { relay } from "../lib/relay";
import type { ShellView } from "../lib/types";

/** Screen 1: what the relay does, and Sign in with Long Lines. */
export function SignedOutScreen({ view, run }: { view: ShellView; run: Run }) {
  const failure = view.pairing.state === "failed" ? copy.signedOut.failures[view.pairing.failure] : null;
  const starting = view.pairing.state === "starting";
  return (
    <Stack component="main" spacing={4} sx={{ px: 5, py: 6 }}>
      <Box>
        <Typography variant="h5" component="h2" gutterBottom>
          {copy.signedOut.heading}
        </Typography>
        <Typography variant="body1" color="text.secondary">
          {copy.signedOut.body}
        </Typography>
      </Box>

      {failure && (
        <Alert severity="error">
          <AlertTitle>{failure.title}</AlertTitle>
          {failure.text}
        </Alert>
      )}

      {!failure && (
        <Stack component="ol" spacing={2.5} sx={{ m: 0, p: 0, listStyle: "none" }}>
          {copy.signedOut.steps(view.settings.port).map((step, i) => (
            <Stack component="li" key={step.title} direction="row" spacing={2} sx={{ alignItems: "flex-start" }}>
              <StepNumber aria-hidden>{i + 1}</StepNumber>
              <Box>
                <Typography variant="body1">{step.title}</Typography>
                <Typography variant="body2" color="text.secondary">
                  {step.text}
                  {i === 1 && (
                    <>
                      {" "}
                      <Link component="button" variant="body2" onClick={() => run(relay.openSetupDocs)}>
                        {copy.signedOut.seeHow}
                      </Link>
                    </>
                  )}
                </Typography>
              </Box>
            </Stack>
          ))}
        </Stack>
      )}

      <Stack spacing={1.5} sx={{ alignItems: "flex-start" }}>
        <Button variant="contained" size="large" disabled={starting} onClick={() => run(relay.startSignIn)}>
          {copy.signedOut.signIn}
        </Button>
        {!failure && (
          <Typography variant="body2" color="text.secondary">
            {copy.signedOut.noAccount}{" "}
            <Link component="button" variant="body2" onClick={() => run(relay.openSignUp)}>
              {copy.signedOut.createAccount}
            </Link>
          </Typography>
        )}
      </Stack>
    </Stack>
  );
}
