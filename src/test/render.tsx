import { ThemeProvider } from "@mui/material/styles";
import { render } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactElement } from "react";
import { theme } from "../theme";

/** Renders inside the app's theme, with a user-event session. */
export function renderWithTheme(ui: ReactElement) {
  const user = userEvent.setup();
  return { user, ...render(<ThemeProvider theme={theme}>{ui}</ThemeProvider>) };
}
