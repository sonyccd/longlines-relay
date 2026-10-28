import ArrowDropDownIcon from "@mui/icons-material/ArrowDropDown";
import LogoutIcon from "@mui/icons-material/Logout";
import PersonIcon from "@mui/icons-material/Person";
import Avatar from "@mui/material/Avatar";
import Box from "@mui/material/Box";
import Button from "@mui/material/Button";
import Divider from "@mui/material/Divider";
import ListItemIcon from "@mui/material/ListItemIcon";
import ListItemText from "@mui/material/ListItemText";
import Menu from "@mui/material/Menu";
import MenuItem from "@mui/material/MenuItem";
import Skeleton from "@mui/material/Skeleton";
import Typography from "@mui/material/Typography";
import { useState } from "react";
import type { Run } from "../App";
import { copy } from "../copy";
import { relay } from "../lib/relay";
import type { Account } from "../lib/types";

/**
 * The header's account button: the callsign, and a menu with the callsign, the email and Sign
 * out. `account` is null until `GET /me` answers (for example, offline at launch); Sign out works
 * either way.
 */
export function AccountMenu({ account, run }: { account: Account | null; run: Run }) {
  const [anchor, setAnchor] = useState<HTMLElement | null>(null);
  const open = anchor !== null;
  return (
    <>
      <Button
        id="account-button"
        color="inherit"
        aria-label={account ? copy.header.accountMenu(account.callsign) : copy.header.accountMenuLoading}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? "account-menu" : undefined}
        onClick={(event) => setAnchor(event.currentTarget)}
        startIcon={
          <Avatar sx={{ width: 32, height: 32 }} aria-hidden>
            {account ? <Typography variant="body2">{account.callsign.slice(0, 2)}</Typography> : <PersonIcon />}
          </Avatar>
        }
        endIcon={<ArrowDropDownIcon />}
      >
        {account?.callsign}
      </Button>
      <Menu
        id="account-menu"
        anchorEl={anchor}
        open={open}
        onClose={() => setAnchor(null)}
        anchorOrigin={{ vertical: "bottom", horizontal: "right" }}
        transformOrigin={{ vertical: "top", horizontal: "right" }}
        slotProps={{ paper: { sx: { width: 280 } }, list: { "aria-label": copy.header.menu } }}
      >
        <Box sx={{ px: 2, pt: 1, pb: 1.5 }}>
          {account ? (
            <>
              <Typography variant="subtitle2">{account.callsign}</Typography>
              <Typography variant="body2" color="text.secondary" noWrap>
                {account.email}
              </Typography>
            </>
          ) : (
            <>
              <Skeleton width={80} />
              <Skeleton width={200} />
            </>
          )}
        </Box>
        <Divider sx={{ mb: 1 }} />
        <MenuItem
          onClick={() => {
            setAnchor(null);
            run(relay.signOut);
          }}
        >
          <ListItemIcon>
            <LogoutIcon />
          </ListItemIcon>
          <ListItemText>{copy.header.signOut}</ListItemText>
        </MenuItem>
      </Menu>
    </>
  );
}
