import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { HandButton, HandCheckbox } from "./components/HandDrawn";
import { ACTION_ITEM, FolderNameEditor, ICON_BUTTON, LibraryActions, LibraryIcon } from "./components/LibraryControls";
import { PenStat } from "./components/PenIcon";
import { DeadlineForm } from "./DeckView";
import { TEXT_FIELD } from "./styles";
import type { DeckSummary, FolderRecord, HomePage, Library } from "./types";

interface Props {
  library: Library | null;
  decks: DeckSummary[];
  folders: FolderRecord[];
  flaggedCount: number;
  onOpen: (page: HomePage) => void;
}

export default function LibraryView({ library, decks, folders, flaggedCount, onOpen }: Props) {
  const [creating, setCreating] = useState(false);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [deadlineFolder, setDeadlineFolder] = useState<string | null>(null);
  const [movingDeck, setMovingDeck] = useState<string | null>(null);
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [recentNames, setRecentNames] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  const createButton = useRef<HTMLButtonElement>(null);
  const folderButtons = useRef(new Map<string, HTMLButtonElement>());
  const deckButtons = useRef(new Map<string, HTMLButtonElement>());
  const focusName = useRef<string | null>(null);
  // Keep a saved folder where its inline draft appeared for this visit.
  const orderedFolders = [...folders].sort((a, b) => {
    const rank = (name: string) => {
      const index = recentNames.indexOf(name);
      return index < 0 ? recentNames.length : index;
    };
    return rank(a.name) - rank(b.name);
  });

  useEffect(() => {
    const folder = folders.find((item) => item.name === focusName.current);
    if (folder && !creating && !renaming) {
      folderButtons.current.get(folder.id)?.focus();
      focusName.current = null;
    }
  }, [folders, creating, renaming]);

  const act = async (action: Record<string, unknown>) => {
    if (busyRef.current) return false;
    busyRef.current = true;
    setError(null);
    setBusy(true);
    try {
      await invoke("organize_library", { action });
      return true;
    } catch (message) {
      setError(String(message));
      return false;
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  };

  const expand = (id: string) => setCollapsed((previous) => {
    const next = new Set(previous); next.delete(id); return next;
  });
  const cancelCreate = () => {
    setCreating(false); setError(null);
    requestAnimationFrame(() => createButton.current?.focus());
  };
  const finishMove = (id: string) => {
    setMovingDeck(null);
    requestAnimationFrame(() => deckButtons.current.get(id)?.focus());
  };
  const cancelRename = (id: string) => {
    setRenaming(null); setError(null);
    requestAnimationFrame(() => folderButtons.current.get(id)?.focus());
  };

  const row = (deck: DeckSummary) => (
    <li key={deck.id} className="border-t border-neutral-200">
      <div className="flex items-center gap-3 py-3">
        <HandCheckbox checked={deck.enabled} disabled={busy}
          aria-label={`Include ${deck.name} in overlay`}
          onChange={(event) => void act({ kind: "selectDeck", deck: deck.id, enabled: event.target.checked })} />
        <button type="button" ref={(element) => {
          if (element) deckButtons.current.set(deck.id, element); else deckButtons.current.delete(deck.id);
        }} onClick={() => onOpen({ deckId: deck.id })}
          className="min-w-0 flex-1 cursor-pointer text-left outline-offset-2 hover:underline focus-visible:outline-2 focus-visible:outline-emerald-700">
          <span className="block text-lg font-medium wrap-anywhere">{deck.name}</span>
          <span className="mt-0.5 flex flex-wrap items-center gap-x-4 gap-y-1 text-sm text-neutral-600">
            <PenStat drawing="cards" value={deck.cardCount} label={`${deck.cardCount} cards`} />
            <PenStat drawing="clock" value={deck.due} label={`${deck.due} due`} />
            <PenStat drawing="sparkle" value={deck.unseen} label={`${deck.unseen} unseen`} />
            <PenStat drawing="ghost" label={deck.overlayEnabled ? "Overlay on" : "Overlay off"}
              className={deck.overlayEnabled ? "" : "text-neutral-300"} />
            {deck.mode === "deadline" && (
              <PenStat drawing="calendar" className="text-emerald-700"
                label={`${deck.deadlineInherited ? "Folder" : "Deck"} deadline`} />
            )}
            {deck.mode === "expired" && <PenStat drawing="calendar" label="Exam passed" className="text-neutral-400" />}
          </span>
        </button>
        <LibraryActions label={`Actions for ${deck.name}`} disabled={busy}>
          {(close) => <button type="button" className={ACTION_ITEM} onClick={() => {
            close(); setMovingDeck(deck.id);
          }}>Move to…</button>}
        </LibraryActions>
      </div>
      {movingDeck === deck.id && (
        <div className="mb-3 ml-11 flex flex-wrap items-center gap-2" onKeyDown={(event) => {
          if (event.key === "Escape" && !busy) { event.preventDefault(); finishMove(deck.id); }
        }}>
          <label className="flex min-w-0 flex-1 items-center gap-3 text-sm">
            Move to
            <select autoFocus aria-label={`Move ${deck.name} to folder`} className={`${TEXT_FIELD} min-w-0 flex-1`}
              disabled={busy} value={deck.folderId ?? ""} onChange={(event) => {
                const folder = event.target.value || null;
                void act({ kind: "moveDeck", deck: deck.id, folder }).then((saved) => {
                  if (saved) { if (folder) expand(folder); finishMove(deck.id); }
                });
              }}>
              <option value="">Unfiled</option>
              {folders.map((folder) => <option key={folder.id} value={folder.id}>{folder.name}</option>)}
            </select>
          </label>
          <button type="button" className={ICON_BUTTON} disabled={busy} aria-label="Cancel moving deck"
            onClick={() => finishMove(deck.id)}><LibraryIcon name="close" /></button>
        </div>
      )}
    </li>
  );

  return (
    <section className="flex min-h-0 flex-1 flex-col gap-3" aria-label="Library">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-2">
          <h2 className="text-2xl font-semibold">Library</h2>
          <button ref={createButton} type="button" className={ICON_BUTTON} disabled={busy || creating}
            aria-label="New folder" title="New folder" onClick={() => {
              setRenaming(null); setError(null); setCreating(true);
            }}><LibraryIcon name="folderPlus" className="size-6" /></button>
        </div>
        <div className="flex flex-wrap gap-3">
          <HandButton seed={24} onClick={() => onOpen("flagged")}>Needs explanation ({flaggedCount})</HandButton>
          <HandButton seed={25} onClick={() => onOpen("import")}>Import deck</HandButton>
          <LibraryActions label="Library actions" disabled={busy}>
            {(close) => <button type="button" className={ACTION_ITEM} onClick={() => {
              close(); void invoke("open_flashcards_folder").catch((message: unknown) => setError(String(message)));
            }}>Open decks folder</button>}
          </LibraryActions>
        </div>
      </div>
      {error && <p role="alert" className="text-sm text-rose-700">{error}</p>}
      <div className="min-h-0 flex-1 overflow-y-auto pr-2">
        {!library && <p>Loading decks...</p>}
        {creating && <div className="mb-3 rounded-lg border border-neutral-200 px-4 py-3">
          <FolderNameEditor busy={busy} onCancel={cancelCreate} onSave={(name) => {
            focusName.current = name;
            void act({ kind: "createFolder", name }).then((saved) => {
              if (saved) { setRecentNames((names) => [name, ...names]); setCreating(false); }
              else focusName.current = null;
            });
          }} />
        </div>}
        {orderedFolders.map((folder) => {
          const members = decks.filter((deck) => deck.folderId === folder.id);
          const open = !collapsed.has(folder.id);
          return (
            <section key={folder.id} aria-label={folder.name} className="mb-3 rounded-lg border border-neutral-200">
              <div className="flex items-center gap-2 px-3 py-2">
                {renaming === folder.id ? (
                  <FolderNameEditor initialName={folder.name} busy={busy} onCancel={() => cancelRename(folder.id)} onSave={(name) => {
                    focusName.current = name;
                    void act({ kind: "renameFolder", id: folder.id, name }).then((saved) => {
                      if (saved) {
                        setRecentNames((names) => names.map((previous) => previous === folder.name ? name : previous));
                        setRenaming(null);
                      } else focusName.current = null;
                    });
                  }} />
                ) : <>
                  <button type="button" ref={(element) => {
                    if (element) folderButtons.current.set(folder.id, element); else folderButtons.current.delete(folder.id);
                  }} aria-expanded={open} aria-controls={`folder-${folder.id}`}
                    className="flex min-w-0 flex-1 cursor-pointer items-center gap-3 rounded-md py-2 text-left outline-emerald-700 focus-visible:outline-2"
                    onClick={() => setCollapsed((previous) => {
                      const next = new Set(previous);
                      if (next.has(folder.id)) next.delete(folder.id); else next.add(folder.id);
                      return next;
                    })}>
                    <LibraryIcon name="chevron" className={`size-4 ${open ? "rotate-90" : ""}`} />
                    <LibraryIcon name="folder" className={`size-6 ${folder.enabled ? "text-ink" : "text-neutral-400"}`} />
                    <span className={`min-w-0 text-lg font-semibold wrap-anywhere ${folder.enabled ? "" : "text-neutral-500"}`}>{folder.name}</span>
                    <span className="shrink-0 text-sm tabular-nums text-neutral-500">{members.length} {members.length === 1 ? "deck" : "decks"}</span>
                  </button>
                  <HandCheckbox seed={92} checked={folder.enabled} disabled={busy}
                    aria-label={`Include ${folder.name} in overlay`} title={folder.enabled ? "Pause folder in overlay" : "Enable folder in overlay"}
                    onChange={(event) => void act({ kind: "selectFolder", id: folder.id, enabled: event.target.checked })} />
                  <LibraryActions label={`Actions for folder ${folder.name}`} disabled={busy}>
                    {(close) => <>
                      <button type="button" className={ACTION_ITEM} onClick={() => {
                        close(); setCreating(false); setRenaming(folder.id); setError(null);
                      }}>Rename</button>
                      <button type="button" className={ACTION_ITEM} onClick={() => {
                        close(); expand(folder.id); setDeadlineFolder(folder.id);
                      }}>Deadline…</button>
                      <button type="button" className={ACTION_ITEM} onClick={() => {
                        close(); void act({ kind: "selectFolder", id: folder.id, enabled: !folder.enabled });
                      }}>{folder.enabled ? "Pause in overlay" : "Enable in overlay"}</button>
                      {members.length === 0 && <button type="button" className={`${ACTION_ITEM} text-rose-700`} onClick={() => {
                        close(); void act({ kind: "deleteFolder", id: folder.id }).then((saved) => { if (saved) createButton.current?.focus(); });
                      }}>Delete empty folder</button>}
                    </>}
                  </LibraryActions>
                </>}
              </div>
              <div id={`folder-${folder.id}`} hidden={!open} className="mr-4 mb-2 ml-9 border-l border-neutral-200 pl-4 sm:ml-12">
                {deadlineFolder === folder.id && <section aria-label={`Deadline for ${folder.name}`} className="mb-4 rounded-md bg-neutral-50 p-4">
                  <div className="mb-3 flex items-center justify-between gap-3">
                    <h3 className="text-lg font-semibold">Folder deadline</h3>
                    <button type="button" className={ICON_BUTTON} aria-label="Close folder deadline" onClick={() => setDeadlineFolder(null)}><LibraryIcon name="close" /></button>
                  </div>
                  <DeadlineForm key={`${folder.id}-${folder.deadline?.exam.utcMs}-${folder.deadline?.enabled}`} folderId={folder.id}
                    deck={{ id: folder.id, deadline: folder.deadline, mode: folder.deadline?.enabled ? "deadline" : "ordinary" }} onSaved={() => { setDeadlineFolder(null); setError(null); }} />
                </section>}
                <ul>{members.map(row)}</ul>
                <button type="button" className="my-1 inline-flex min-h-10 cursor-pointer items-center gap-2 rounded-md px-2 text-neutral-500 outline-emerald-700 hover:bg-neutral-100 hover:text-ink focus-visible:outline-2"
                  aria-label={`Import deck into ${folder.name}`} title={`Import deck into ${folder.name}`}
                  onClick={() => onOpen({ importFolderId: folder.id })}>
                  <LibraryIcon name="deckPlus" />
                  {members.length === 0 && <span className="text-sm">Import a deck</span>}
                </button>
              </div>
            </section>
          );
        })}
        {decks.some((deck) => !deck.folderId) && <section>
          <h3 className="py-2 text-sm font-medium text-neutral-500">Unfiled</h3>
          <ul>{decks.filter((deck) => !deck.folderId).map(row)}</ul>
        </section>}
        {library && decks.length === 0 && folders.length === 0 && !creating && <p className="py-4 text-neutral-600">Import a deck to start studying.</p>}
        {[...(library?.errors ?? []), ...(library?.warnings ?? [])].map(({ file, message }) => (
          <p key={`${file}-${message}`} className="py-2 text-sm whitespace-pre-line text-amber-800">{file}: {message}</p>
        ))}
      </div>
    </section>
  );
}
