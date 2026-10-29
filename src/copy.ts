// Every word the window shows. Strings come from Brad's brief and the page 2 mockups; the ones
// marked "Invented" are not in either and are Brad's to reword. Row statuses and the feed chip's
// label come from the core (crates/relay-core/src/status.rs and state.rs).

export const copy = {
  appName: "Long Lines Relay",
  setUpLogging: "Set up logging software",

  signedOut: {
    heading: "Sign in to start relaying",
    body: "The relay sends the contacts you log to Long Lines as spots. Sign in with your Long Lines account to connect this computer.",
    steps: (port: number) => [
      {
        title: "Sign in with Long Lines",
        text: "Your browser opens so you can approve this computer. You don't type a password here.",
      },
      { title: "Set up your logging software", text: `Point its contact broadcast at this computer, port ${port}.` },
      { title: "Log contacts as usual", text: "Each contact shows up here and on Long Lines a moment later." },
    ],
    seeHow: "See how",
    signIn: "Sign in with Long Lines",
    noAccount: "No account?",
    createAccount: "Create one on Long Lines",
    failures: {
      expired: {
        title: "The code expired",
        text: "It wasn't approved within 10 minutes. Sign in again to get a new code.",
      },
      // Invented: the mockups draw only the expired code.
      denied: {
        title: "Sign-in was denied",
        text: "Deny was selected in the browser, so this computer isn't connected. Sign in again to get a new code.",
      },
      // Invented.
      unreachable: {
        title: "Couldn't reach Long Lines",
        text: "Check this computer's internet connection, then sign in again.",
      },
      // Invented.
      refused: {
        title: "Long Lines didn't accept the sign-in",
        text: "Sign in again. If it keeps happening, check for a newer version of the relay.",
      },
    },
  },

  signingIn: {
    heading: "Finish signing in in your browser",
    body: "Long Lines opened in your browser. Check that it shows this code, then select Allow.",
    yourCode: "Your code",
    expiresIn: (left: string) => `Expires in ${left}`,
    waiting: "Waiting for you to approve",
    cancel: "Cancel",
    openAgain: "Open browser again",
    browserDidntOpen: "Browser didn't open? Go to",
    enterTheCode: "and enter the code.",
  },

  // The signed-in header: the Connected chip and the account menu (the RelayTab* mockups).
  header: {
    connected: "Connected",
    accountMenu: (callsign: string) => `Account menu, ${callsign}`,
    // Invented: the mockups always know the callsign; before `GET /me` answers the button has none.
    accountMenuLoading: "Account menu",
    menu: "Account",
    signOut: "Sign out",
  },

  tabs: {
    label: "Relay",
    contacts: "Contacts",
    dxCluster: "DX cluster",
  },

  feed: {
    heading: "Logging feed",
    lastContact: (ago: string) => `Last contact ${ago}`,
    nothingYet: "Nothing received yet",
    listening: (port: number) => `Listening on UDP port ${port}`,
    portInUse: (port: number) => `Port ${port} is in use by another program`,
    // Invented: the brief asks for a way to change the port but gives no words.
    portInUseHelp: "Choose another port here and in your logging software.",
    waitingHelp: (port: number) =>
      `Turn on contact broadcasts in your logging software and send them to this computer on port ${port}.`,
    mismatchTitle: (mycall: string) => `Contacts are logged as ${mycall}`,
    mismatchText: (account: string, mycall: string) =>
      `This account is ${account}, so contacts logged as ${mycall} are ignored. Change the callsign in your logging software, or sign in with the account for ${mycall}.`,
    testModeOn: "Test mode is on. Contacts aren't being sent to Long Lines.",
    testMode: "Test mode",
    testModeHelp: "Test mode shows contacts here without sending them to Long Lines.",
  },

  contacts: {
    heading: "Contacts seen",
    subtitle: "Everything the relay receives, newest first.",
    subtitleTestMode: "Everything the relay receives, newest first. In test mode it shows what would have happened.",
    empty: "Contacts you log will show up here.",
  },

  // The DX cluster tab (the RelayDxOff, RelayDxListening and RelayDxRunning mockups). The chip's
  // label comes from the core.
  cluster: {
    heading: "Local DX cluster",
    offText:
      "Turn this on to show Long Lines spots in your logging software's bandmap. Your logging software connects to this computer as if it were a DX cluster.",
    offNote: "Nothing listens on this computer while it's off.",
    noSpots: "No spots sent yet",
    lastSpot: (ago: string, today: number) =>
      `Last spot sent ${ago} · ${today} ${today === 1 ? "spot" : "spots"} today`,
    programs: (n: number) => (n === 0 ? "No programs connected" : `${n} ${n === 1 ? "program" : "programs"} connected`),
    noDestination:
      "No destination sends spots to this computer yet. In Long Lines, add a Local DX cluster destination and choose this relay.",
    addDestination: "Add destination",
    details: "Connection details for your logging software",
    address: "Address",
    port: "Port",
    thisComputer: "This computer",
    otherComputers: "Other computers",
    copyAddress: (address: string) => `Copy address ${address}`,
    copyPort: (port: number) => `Copy port ${port}`,
    copyCallsign: (callsign: string) => `Copy callsign ${callsign}`,
    // Invented: the mockups draw no confirmation after a copy.
    copied: "Copied",
    lan: "Let other computers on this network connect",
    lanNote: "Anyone on your network can connect while this is on.",
    sendTest: "Send test spot",
    changePort: "Change port",
    // Invented: the mockups draw the default port only.
    portMoved: (saved: number, bound: number) =>
      `Port ${saved} is in use by another program, so the relay listens on port ${bound}. Use port ${bound} in your logging software.`,
    // The title is the plan's; the rest is invented.
    scopeMissingTitle: "Sign in again to use the DX cluster",
    scopeMissingText:
      "This computer was signed in before the relay could receive spots. Sign in again and select Allow in your browser.",
    signInAgain: "Sign in again",
    // Invented: the mockups draw the stream connected only.
    retrying: (left: string) => `Couldn't reach Long Lines for spots. Trying again in ${left}.`,
    sent: {
      heading: "Spots sent",
      subtitle: "What the relay sent to your logging software, newest first.",
      test: "Test",
    },
  },

  // "Set up logging software" on the DX cluster tab (the RelayDxSetup mockup, without its
  // "If 127.0.0.1 won't connect" line: plan decision 6).
  dxSetup: {
    back: "Back",
    intro:
      "Your logging software connects to the relay as if it were a DX cluster. Matched spots then show up in its bandmap.",
    software: "Logging software",
    n1mm: "N1MM Logger+",
    other: "Other software",
    addressHeading: "Cluster address",
    callsign: "Callsign, if asked",
    stepsHeading: "Steps",
    steps: {
      n1mm: ["Open the Telnet window in N1MM Logger+.", "Add a new cluster with the address and port above."],
      // Invented: the mockup draws the N1MM Logger+ tab only.
      other: [
        "Open the DX cluster or Telnet settings in your logging software.",
        "Add a new cluster with the address and port above.",
      ],
    },
    connect: "Connect to it.",
    // Plan decision 6: shown only while other computers may connect.
    lanAddress: (address: string) => `Other computers on your network use ${address}.`,
    check: "Check that it works.",
    checkText: "Send a test spot. It should show up in the bandmap.",
    // Invented: the mockup draws no confirmation.
    testSent: "Test spot sent.",
    testFailed: "Couldn't send the test spot. Check that the DX cluster is on.",
    filters: {
      n1mm: "N1MM Logger+ applies its own spot filters and band limits. If a spot doesn't show up, check those first.",
      // Invented.
      other:
        "Your logging software applies its own spot filters and band limits. If a spot doesn't show up, check those first.",
    },
  },

  // Invented: the brief lists the settings (UDP port, "Start when I log in") but no screen. The
  // tab mockups draw none either; the card stays at the foot of the Contacts tab.
  settings: {
    heading: "Settings",
    port: "UDP port",
    changePort: "Change port",
    badPort: "Enter a port from 1 to 65535.",
    autostart: "Start when I log in",
    version: (v: string) => `Version ${v}`,
  },

  // Invented: the brief asks for self-update but draws no screen for it.
  update: {
    available: (v: string) => `Version ${v} of the relay is available.`,
    install: "Install and restart",
    installing: (v: string) => `Installing version ${v}. The relay restarts when it's done.`,
    failed: (v: string) => `Couldn't install version ${v}.`,
    retry: "Try again",
    // Invented: a copy the updater can't replace (an .rpm or a bare binary) gets a link to the release.
    manual: (v: string) => `Version ${v} of the relay is available. Download the new package to install it.`,
    download: "Download",
  },
} as const;
