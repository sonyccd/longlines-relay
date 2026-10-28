import Box from "@mui/material/Box";
import LinearProgress from "@mui/material/LinearProgress";
import { AppHeader } from "./components/AppHeader";
import type { ShellView } from "./lib/types";
import { useRelay } from "./lib/useRelay";
import { MainScreen } from "./screens/MainScreen";
import { SignedOutScreen } from "./screens/SignedOutScreen";
import { SigningInScreen } from "./screens/SigningInScreen";

/** Runs a command that answers with a view, and shows that view at once. */
export type Run = (command: () => Promise<ShellView | void>) => void;

export function App() {
  const [view, setView] = useRelay();
  const run: Run = (command) => {
    void command()
      .then((next) => {
        if (next) setView(next);
      })
      .catch((error: unknown) => {
        // The shell logs failures; the next `state` event shows where things stand.
        console.error(error);
      });
  };

  return (
    <Box sx={{ minHeight: "100vh", display: "flex", flexDirection: "column" }}>
      {/* Signed in, the main screen draws the header, since its setup screen has its own. */}
      {view?.signed_in ? (
        <MainScreen view={view} run={run} />
      ) : (
        <>
          <AppHeader signedIn={false} account={null} run={run} />
          {view === null ? (
            <LinearProgress aria-label="Loading" />
          ) : view.pairing.state === "waiting" ? (
            <SigningInScreen pairing={view.pairing} run={run} />
          ) : (
            <SignedOutScreen view={view} run={run} />
          )}
        </>
      )}
    </Box>
  );
}
