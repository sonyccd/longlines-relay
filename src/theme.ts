import { createTheme } from "@mui/material/styles";
import type { CSSProperties } from "react";

declare module "@mui/material/styles" {
  interface TypographyVariants {
    code: CSSProperties;
  }
  interface TypographyVariantsOptions {
    code?: CSSProperties;
  }
}

declare module "@mui/material/Typography" {
  interface TypographyPropsVariantOverrides {
    code: true;
  }
}

/**
 * The stock MUI dark theme, as the Long Lines web app uses, plus one variant for the pairing
 * code (Roboto Mono 36/500, 4 px apart, as the signing-in mockup draws it).
 */
export const theme = createTheme({
  palette: { mode: "dark" },
  typography: {
    code: {
      fontFamily: "'Roboto Mono', monospace",
      fontSize: 36,
      fontWeight: 500,
      lineHeight: 1.4,
      letterSpacing: 4,
    },
  },
});
