import AppBar from "@mui/material/AppBar";
import Button from "@mui/material/Button";
import Toolbar from "@mui/material/Toolbar";
import Typography from "@mui/material/Typography";
import OpenInNewIcon from "@mui/icons-material/OpenInNew";
import { copy } from "../copy";

export function AppHeader({ onSetUp }: { onSetUp: () => void }) {
  return (
    <AppBar position="static">
      <Toolbar sx={{ justifyContent: "space-between", gap: 2 }}>
        <Typography variant="h6" component="h1">
          {copy.appName}
        </Typography>
        <Button color="primary" endIcon={<OpenInNewIcon />} onClick={onSetUp}>
          {copy.setUpLogging}
        </Button>
      </Toolbar>
    </AppBar>
  );
}
