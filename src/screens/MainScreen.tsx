import Box from "@mui/material/Box";
import Stack from "@mui/material/Stack";
import Tab from "@mui/material/Tab";
import Tabs from "@mui/material/Tabs";
import { useState, type ReactNode } from "react";
import type { Run } from "../App";
import { AppHeader } from "../components/AppHeader";
import { ContactsCard } from "../components/ContactsCard";
import { DxClusterCard } from "../components/DxClusterCard";
import { FeedCard } from "../components/FeedCard";
import { SettingsCard } from "../components/SettingsCard";
import { SpotsSentList } from "../components/SpotsSentList";
import { UpdateBanner } from "../components/UpdateBanner";
import { UpstreamAlert } from "../components/UpstreamAlert";
import { copy } from "../copy";
import type { ShellView } from "../lib/types";
import { DxSetupScreen } from "./DxSetupScreen";

type TabId = "contacts" | "dx";

/** A tab's panel. Only the selected one renders its content. */
function Panel({ id, selected, children }: { id: TabId; selected: TabId; children: ReactNode }) {
  return (
    <Box
      component="main"
      id={`panel-${id}`}
      role="tabpanel"
      aria-labelledby={`tab-${id}`}
      hidden={id !== selected}
      sx={{ flexGrow: 1, px: 3, pt: 2, pb: 3 }}
    >
      {id === selected && children}
    </Box>
  );
}

/**
 * Signed in: the header, then the Contacts tab (the logging feed, Contacts seen and the
 * settings) and the DX cluster tab (the local DX cluster and Spots sent). "Set up logging
 * software" on the DX cluster tab replaces all of it with the setup screen until Back, which
 * returns to the DX cluster tab.
 */
export function MainScreen({ view, run }: { view: ShellView; run: Run }) {
  const [tab, setTab] = useState<TabId>("contacts");
  const [settingUp, setSettingUp] = useState(false);
  if (settingUp) {
    return <DxSetupScreen cluster={view.cluster} account={view.account} run={run} onBack={() => setSettingUp(false)} />;
  }
  return (
    <>
      <AppHeader signedIn account={view.account} upstream={view.upstream} run={run} />
      <Box sx={{ px: 1, borderBottom: 1, borderColor: "divider" }}>
        <Tabs value={tab} onChange={(_, next: TabId) => setTab(next)} aria-label={copy.tabs.label}>
          <Tab value="contacts" label={copy.tabs.contacts} id="tab-contacts" aria-controls="panel-contacts" />
          <Tab value="dx" label={copy.tabs.dxCluster} id="tab-dx" aria-controls="panel-dx" />
        </Tabs>
      </Box>
      <Panel id="contacts" selected={tab}>
        <Stack spacing={2}>
          <UpdateBanner update={view.update} run={run} />
          <UpstreamAlert upstream={view.upstream} />
          <FeedCard feed={view.feed} account={view.account} run={run} />
          <ContactsCard rows={view.stream} testMode={view.feed.test_mode} />
          <SettingsCard settings={view.settings} version={view.version} run={run} />
        </Stack>
      </Panel>
      <Panel id="dx" selected={tab}>
        <Stack spacing={2}>
          <DxClusterCard
            cluster={view.cluster}
            sent={view.sent}
            settings={view.settings}
            run={run}
            onSetUp={() => setSettingUp(true)}
          />
          {view.cluster.enabled && view.sent.length > 0 && <SpotsSentList rows={view.sent} />}
        </Stack>
      </Panel>
    </>
  );
}
