import { styled } from "@mui/material/styles";

/** The numbered circle before each step, on the signed-out and setup screens. */
export const StepNumber = styled("span")(({ theme }) => ({
  flex: "none",
  width: 28,
  height: 28,
  borderRadius: "50%",
  border: `1px solid ${theme.palette.divider}`,
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  ...theme.typography.subtitle2,
}));
