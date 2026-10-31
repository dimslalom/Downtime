const OPEN = "death";
const HIDE = "respawn";

// Valorant has no respawn event: a dead player is out until the next buy phase.
const VALORANT_ROUND_OVER = new Set(["shopping", "game_end"]);

function plain(value) {
  return String(value).replace(/"/g, "");
}

/**
 * Each game maps a GEP game event or info update to a Downtime event, or null to
 * ignore it. Deadlock has no respawn event, so only the hide key or the end of
 * the match hides the overlay there.
 */
const GAMES = {
  21640: {
    name: "Valorant",
    features: ["game_info", "me", "match_info", "kill", "death"],
    toDowntimeEvent(data) {
      if (data.key === "death") return OPEN;
      if (data.key === "match_end") return HIDE;
      if (data.key === "round_phase" && VALORANT_ROUND_OVER.has(plain(data.value))) return HIDE;
      return null;
    },
  },
  10844: {
    name: "Overwatch 2",
    features: ["game_info", "match_info", "death"],
    toDowntimeEvent(data) {
      if (data.key === "death") return OPEN;
      if (["respawn", "revive", "round_start", "match_end"].includes(data.key)) return HIDE;
      return null;
    },
  },
  24482: {
    name: "Deadlock",
    features: ["game_info", "match_info"],
    toDowntimeEvent(data) {
      if (data.key === "death") return OPEN;
      if (data.key === "match_end") return HIDE;
      return null;
    },
  },
};

module.exports = { GAMES };
