import { invoke } from "@tauri-apps/api/core";
import { useState } from "react";
import GhostWithCard from "./components/GhostWithCard";
import { HandButton, HandPanel } from "./components/HandDrawn";
import Lettered from "./components/Lettered";
import DeckView from "./DeckView";
import FlaggedView from "./FlaggedView";
import { useHome, useHotkeys, useLibraryValue } from "./hooks";
import ImportView from "./ImportView";
import { formatHotkey, type Hotkeys } from "./services/hotkey";
import SettingsView from "./SettingsView";
import { KEYCAP, PAGE } from "./styles";
import LibraryView from "./LibraryView";
import type { HomePage } from "./types";

function HotkeyPanel({ hotkeys }: { hotkeys: Hotkeys | null }) {
  const open = hotkeys ? formatHotkey(hotkeys.openHotkey) : "";
  const hide = hotkeys ? formatHotkey(hotkeys.hideKey) : "";

  return (
    <HandPanel seed={41} className="flex items-center justify-between gap-6 px-8 py-5 short:py-3">
      <div className="flex flex-1 flex-col items-start gap-3 short:flex-row short:items-center short:justify-between short:gap-6">
        <div className="flex flex-col gap-1">
          <h2 className="text-2xl font-semibold text-ink">Study while you wait</h2>
          {!hotkeys && <p className="text-base text-neutral-600">Loading your keys...</p>}
          {hotkeys?.openRegistered === false && (
            <p role="alert" className="text-base text-rose-700">
              {open} is taken by another app. Choose a different key in Settings.
            </p>
          )}
          {hotkeys?.openRegistered && (
            <p className="text-base leading-8 text-neutral-600">
              Press <kbd className={KEYCAP}>{open}</kbd> to open or close your cards.{" "}
              <kbd className={KEYCAP}>{hide}</kbd> closes them too.
            </p>
          )}
        </div>
        <div className="flex items-center gap-4 short:flex-row-reverse">
          <HandButton
            seed={7}
            variant="primary"
            onClick={() => invoke("fire_mock_event", { event: "death" }).catch(console.error)}
          >
            Open overlay
          </HandButton>
          {hotkeys?.openRegistered && /^F\d+$/.test(open) && (
            <p className="text-sm text-neutral-600 short:text-right short:text-xs">
              On a laptop you may need the Fn key too.
            </p>
          )}
        </div>
      </div>
      <GhostWithCard className="h-28 shrink-0 text-ink short:hidden" />
    </HandPanel>
  );
}

export default function HomeView() {
  const library = useLibraryValue();
  const hotkeys = useHotkeys();
  const home = useHome();
  const [page, setPage] = useState<HomePage>("home");
  const goHome = () => setPage("home");

  if (page === "settings") {
    return <SettingsView hotkeys={hotkeys} home={home} onBack={goHome} />;
  }
  if (page === "import") return <ImportView onBack={goHome} />;
  if (page === "flagged") return <FlaggedView cards={home?.flagged ?? []} onBack={goHome} />;
  if (typeof page === "object" && "importFolderId" in page) {
    return <ImportView onBack={goHome} targetFolderId={page.importFolderId} targetFolder={home?.folders.find(({ id }) => id === page.importFolderId)} />;
  }
  if (typeof page === "object" && "deckId" in page) {
    const deck = home?.decks.find(({ id }) => id === page.deckId);
    if (deck && home) return <DeckView deck={deck} measured={home.measured} onBack={goHome} />;
  }

  return (
    <main className={PAGE}>
      <header className="flex items-start justify-between gap-6">
        <div className="flex flex-col gap-2">
          <Lettered word="downtime" as="h1" className="h-14 text-ink" />
          <p className="text-sm text-neutral-600">
            Closing this window keeps Downtime running in the system tray.
          </p>
        </div>
        <HandButton seed={12} onClick={() => setPage("settings")}>
          Settings
        </HandButton>
      </header>
      <HotkeyPanel hotkeys={hotkeys} />
      <LibraryView
        library={library}
        decks={home?.decks ?? []}
        folders={home?.folders ?? []}
        flaggedCount={home?.flagged.length ?? 0}
        onOpen={setPage}
      />
    </main>
  );
}
