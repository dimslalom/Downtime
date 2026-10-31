import { invoke } from "@tauri-apps/api/core";
import { useState } from "react";
import { HandButton, HandCheckbox } from "./components/HandDrawn";
import { LibraryIcon } from "./components/LibraryControls";
import { DECK_PROMPT } from "./deckPrompt";
import { PAGE, TEXT_FIELD } from "./styles";
import type { FolderRecord, ImportPreview } from "./types";

function CopyPrompt() {
  const [copied, setCopied] = useState(false);

  const copy = () => {
    navigator.clipboard
      .writeText(DECK_PROMPT)
      .then(() => setCopied(true))
      .catch(console.error);
  };

  return (
    <details className="border-b border-neutral-200 pb-3">
      <summary className="cursor-pointer text-lg font-semibold">Make a deck with an AI</summary>
      <div className="flex flex-col items-start gap-3 pt-3">
        <p className="text-base text-neutral-600">
          Copy this prompt into any AI along with your course material, then paste its reply below.
        </p>
        <HandButton seed={51} onClick={copy}>
          {copied ? "Copied" : "Copy prompt"}
        </HandButton>
        <pre className="max-h-48 w-full overflow-y-auto rounded-md bg-neutral-100 p-3 text-sm whitespace-pre-wrap">
          {DECK_PROMPT}
        </pre>
      </div>
    </details>
  );
}

interface PreviewProps {
  preview: ImportPreview;
  resets: string[];
  onToggleReset: (key: string) => void;
}

function Preview({ preview, resets, onToggleReset }: PreviewProps) {
  return (
    <section className="flex flex-col gap-3 border-t border-neutral-200 pt-3">
      <h2 className="text-lg font-semibold">
        {preview.isUpdate ? "Update" : "New deck"}: {preview.deckName}
      </h2>
      <p className="text-base text-neutral-600">
        {preview.total} cards: {preview.newCards} new, {preview.unchanged} unchanged,{" "}
        {preview.editedUnreviewed + preview.changedWithHistory.length} edited. Saves as{" "}
        {preview.fileName}.
      </p>
      {preview.removedWithHistory > 0 && (
        <p className="text-base text-amber-800">
          {preview.removedWithHistory === 1
            ? "1 reviewed card is missing from this version. Its history is kept and returns if it does."
            : `${preview.removedWithHistory} reviewed cards are missing from this version. Their history is kept and returns if they do.`}
        </p>
      )}
      {preview.warnings.map((warning) => (
        <p key={warning} className="text-base text-amber-800">
          {warning}
        </p>
      ))}
      {preview.changedWithHistory.length > 0 && (
        <div className="flex flex-col gap-2">
          <p className="text-base font-medium">
            These reviewed cards changed. Reset one only if it now tests something different.
          </p>
          {preview.changedWithHistory.map((card) => (
            <label key={card.key} className="flex items-start gap-3 border-t border-neutral-200 py-2">
              <HandCheckbox
                className="mt-1"
                checked={resets.includes(card.key)}
                onChange={() => onToggleReset(card.key)}
              />
              <span className="flex flex-col gap-1 text-base wrap-anywhere">
                <span>
                  {card.oldFront === card.front ? card.front : `${card.oldFront} → ${card.front}`}
                </span>
                <span className="text-neutral-600">
                  {card.oldBack === card.back ? card.back : `${card.oldBack} → ${card.back}`}
                </span>
                <span className="text-sm text-neutral-500">
                  {card.reviews} {card.reviews === 1 ? "review" : "reviews"}. Reset learning state.
                </span>
              </span>
            </label>
          ))}
        </div>
      )}
    </section>
  );
}

export default function ImportView({ onBack, targetFolder, targetFolderId }: {
  onBack: () => void;
  targetFolder?: FolderRecord;
  targetFolderId?: string;
}) {
  const [contents, setContents] = useState("");
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const [resets, setResets] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  const check = () => {
    setError(null);
    setResets([]);
    invoke<ImportPreview>("preview_deck_import", { contents })
      .then(setPreview)
      .catch((message: unknown) => {
        setPreview(null);
        setError(String(message));
      });
  };

  const save = () => {
    if (saving) return;
    setSaving(true);
    setError(null);
    invoke("commit_deck_import", { contents, resetKeys: resets, targetFolderId: targetFolderId ?? null })
      .then(onBack)
      .catch((message: unknown) => setError(String(message)))
      .finally(() => setSaving(false));
  };

  const toggleReset = (key: string) =>
    setResets((keys) => (keys.includes(key) ? keys.filter((other) => other !== key) : [...keys, key]));

  return (
    <main className={PAGE}>
      <header className="flex items-start justify-between gap-6">
        <div className="flex min-w-0 flex-col gap-2">
          <h1 className="text-4xl font-bold">Import a deck</h1>
          {targetFolder && <p className="flex items-center gap-2 text-sm text-neutral-600">
            <LibraryIcon name="folder" /> <span className="wrap-anywhere">{targetFolder.name}</span>
          </p>}
        </div>
        <HandButton seed={12} onClick={onBack} disabled={saving}>
          Back
        </HandButton>
      </header>

      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto pr-2">
        <CopyPrompt />
        <label className="flex flex-col gap-2">
          <span className="text-lg font-semibold">Deck JSON</span>
          <textarea
            disabled={saving}
            value={contents}
            onChange={(event) => {
              setContents(event.target.value);
              setPreview(null);
            }}
            rows={8}
            spellCheck={false}
            placeholder='{"deck_name": "Name", "cards": [{"front": "Q", "back": "A"}]}'
            className={`${TEXT_FIELD} font-mono text-sm`}
          />
        </label>
        {error && (
          <p role="alert" className="text-base whitespace-pre-line text-rose-700">
            {error}
          </p>
        )}
        {preview && <Preview preview={preview} resets={resets} onToggleReset={toggleReset} />}
        <div className="flex gap-3 pb-2">
          <HandButton seed={52} disabled={saving || !contents.trim()} onClick={check}>
            Check deck
          </HandButton>
          <HandButton seed={53} variant="primary" disabled={saving || !preview} onClick={save}>
            {saving ? "Saving…" : preview?.isUpdate ? "Update deck" : "Add deck"}
          </HandButton>
        </div>
      </div>
    </main>
  );
}
