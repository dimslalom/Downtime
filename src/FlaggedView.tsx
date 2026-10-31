import { invoke } from "@tauri-apps/api/core";
import { HandButton } from "./components/HandDrawn";
import { PenStat } from "./components/PenIcon";
import { PAGE } from "./styles";
import type { FlaggedCard } from "./types";

interface FlaggedViewProps {
  cards: FlaggedCard[];
  onBack: () => void;
}

export default function FlaggedView({ cards, onBack }: FlaggedViewProps) {
  const returnToReview = (key: string) => {
    invoke("set_card_flag", { key, flagged: false }).catch(console.error);
  };

  return (
    <main className={PAGE}>
      <header className="flex items-start justify-between gap-6">
        <div className="flex flex-col gap-1">
          <h1 className="text-4xl font-bold">Needs explanation</h1>
          <p className="text-base text-neutral-600">
            These wait behind everything else in rapid review, and still count as gaps in your coverage
            until you return them.
          </p>
        </div>
        <HandButton seed={12} onClick={onBack}>
          Back
        </HandButton>
      </header>

      <ul className="flex min-h-0 flex-1 flex-col overflow-y-auto border-t border-neutral-200 pr-2">
        {cards.length === 0 && (
          <li className="py-4 text-base text-neutral-600">
            Nothing flagged. Press N on a card in the overlay to add one.
          </li>
        )}
        {cards.map((card) => (
          <li
            key={card.key}
            className="flex items-start justify-between gap-6 border-b border-neutral-200 py-3"
          >
            <div className="flex flex-col gap-1 wrap-anywhere">
              <p className="text-lg font-medium">{card.front}</p>
              <p className="text-base text-neutral-700">{card.back}</p>
              <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-sm text-neutral-600">
                <PenStat drawing="cards" value={card.deckName} label={`Deck: ${card.deckName}`} />
                <PenStat drawing="book" value={card.source ?? "No source given"}
                  label={card.source ? `Source: ${card.source}` : "No source given"} />
              </div>
            </div>
            <HandButton seed={61} onClick={() => returnToReview(card.key)}>
              Return to review
            </HandButton>
          </li>
        ))}
      </ul>
    </main>
  );
}
