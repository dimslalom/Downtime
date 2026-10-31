/** Original line drawing: a ghost, waiting out its respawn timer, studying a flashcard. */
export default function GhostWithCard({ className }: { className: string }) {
  return (
    <svg
      aria-hidden
      viewBox="0 0 170 130"
      className={className}
      fill="none"
      stroke="currentColor"
      strokeWidth="2.2"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      {/* body */}
      <path d="M22 118C21 92 19 66 25 46C31 24 58 12 78 22C96 31 100 52 101 72" />
      <path d="M22 118C27 111 31 110 35 117C39 123 44 122 47 115C50 109 56 110 59 117C62 123 69 122 72 115C75 110 80 111 84 116" />
      {/* face */}
      <path d="M48 52C48 49 52 49 52 52C52 55 48 55 48 52Z" fill="currentColor" />
      <path d="M71 50C71 47 75 47 75 50C75 53 71 53 71 50Z" fill="currentColor" />
      <path d="M57 63C59 67 64 67 66 62" />
      {/* arm reaching for the card */}
      <path d="M78 84C86 86 92 84 98 80" />
      {/* flashcard */}
      <path
        d="M97 60C114 57 136 54 153 55C155 72 157 90 158 106C141 108 118 110 103 111C101 94 98 76 97 60Z"
        className="fill-emerald-300"
      />
      <path d="M110 74C121 72 133 71 144 71" />
      <path d="M112 86C120 85 127 84 135 84" />
      {/* a thought about the timer */}
      <path d="M116 38C118 36 121 36 122 38" />
      <path d="M130 24C133 20 139 20 142 24" />
      <path d="M149 8C153 3 160 4 163 9" />
    </svg>
  );
}
