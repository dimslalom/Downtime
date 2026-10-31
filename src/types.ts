export type CardType = "Definition" | "Relationship" | "Exception" | "Method Choice";
export type HomePage = "home" | "settings" | "import" | "flagged" | { deckId: string } | { importFolderId: string };

export interface Card {
  id: string;
  key: string;
  explicitId: boolean;
  cardType: CardType | null;
  front: string;
  back: string;
  source: string | null;
  importance: number | null;
  importanceReason: string | null;
  assessmentEvidence: string | null;
}

export interface Deck {
  id: string;
  file: string;
  name: string;
  cards: Card[];
}

export interface DeckError {
  file: string;
  message: string;
}

/** Mirrors `Library` in src-tauri/src/flashcards.rs. */
export interface Library {
  folder: string;
  decks: Deck[];
  errors: DeckError[];
  warnings: DeckError[];
}

export type Rating = "again" | "hard" | "good" | "easy";
export type Mode = "ordinary" | "deadline" | "expired";
export type Phase = "new" | "learning" | "review" | "relearning";
export type Fit = "fits" | "tight" | "over";

/** The rest of this file mirrors src-tauri/src/study/engine.rs and store.rs. */
export interface CardView {
  retry: boolean;
  attemptId: string;
  key: string;
  cardType: CardType | null;
  front: string;
  back: string;
  source: string | null;
  importance: number;
  importanceReason: string | null;
  flagged: boolean;
}

export interface SessionView {
  deckId: string;
  deckName: string;
  mode: Mode;
  extra: boolean;
  passed: number;
  failed: number;
  remaining: number;
  flipped: boolean;
  card: CardView | null;
  nextDueMs: number | null;
  planPending: boolean;
}

export interface StudyView {
  deckCount: number;
  session: SessionView | null;
}

export interface ExamTime {
  utcMs: number;
  local: string;
  timeZone: string;
  dateOnly: boolean;
}

export interface AdvancedPlanning {
  reliability: number | null;
  targetRecall: number | null;
  maxNewPerDay: number | null;
}

export interface Deadline {
  enabled: boolean;
  exam: ExamTime;
  dailyMinutes: number | null;
  advanced: AdvancedPlanning;
}

export interface DeckEstimate {
  examRecallWeighted: number;
  examRecall: number;
  noStudyRecallWeighted: number;
  unseenNow: number;
  frozen: number;
  expectedIntroduced: number;
  expectedReviews: number;
  expectedMinutes: number;
  todayMinutes: number;
}

export interface PlanView {
  estimate: DeckEstimate;
  expectedUnseenAtExam: number;
  minutesAvailable: number;
  minutesToCoverUnseen: number;
  dailyMinutes: number;
  reliability: number;
  targetRecall: number;
  fit: Fit;
}

export interface DeckSummary {
  folderId: string | null;
  enabled: boolean;
  overlayEnabled: boolean;
  deadlineInherited: boolean;
  id: string;
  name: string;
  file: string;
  cardCount: number;
  seen: number;
  unseen: number;
  due: number;
  flagged: number;
  changed: number;
  orphaned: number;
  mode: Mode;
  deadline: Deadline | null;
  plan: PlanView | null;
}

export interface CardRow {
  key: string;
  cardType: CardType | null;
  front: string;
  back: string;
  source: string | null;
  importance: number;
  suggestedImportance: number | null;
  importanceReason: string | null;
  assessmentEvidence: string | null;
  flagged: boolean;
  changed: boolean;
  phase: Phase;
  reviews: number;
  lapses: number;
  recallNow: number | null;
  dueMs: number | null;
}

export interface FlaggedCard {
  key: string;
  deckName: string;
  front: string;
  back: string;
  source: string | null;
  flaggedMs: number;
}

export interface Measured {
  reviewsToday: number;
  minutesToday: number;
  dailyMinutes: number;
  secondsPerReview: number;
  costReviews: number;
  costsAreMeasured: boolean;
}

export interface ModelView {
  modelVersion: string;
  personalized: boolean;
  fittedMs: number | null;
  fitItems: number;
  limitedData: boolean;
  lastFitError: string | null;
  availableItems: number;
  reviewCount: number;
}

export interface StudySettings {
  desired_retention: number;
  new_per_day: number;
  daily_minutes: number;
}

export interface FolderRecord {
  id: string;
  name: string;
  enabled: boolean;
  deadline: Deadline | null;
}

export interface HomeSnapshot {
  folders: FolderRecord[];
  decks: DeckSummary[];
  flagged: FlaggedCard[];
  measured: Measured;
  model: ModelView;
  settings: StudySettings;
  fitting: boolean;
  folder: string;
}

export interface ChangedCard {
  key: string;
  oldFront: string;
  oldBack: string;
  front: string;
  back: string;
  reviews: number;
}

export interface ImportPreview {
  deckName: string;
  fileName: string;
  isUpdate: boolean;
  total: number;
  newCards: number;
  unchanged: number;
  editedUnreviewed: number;
  changedWithHistory: ChangedCard[];
  removedWithHistory: number;
  warnings: string[];
}
