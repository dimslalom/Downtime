const assert = require("node:assert/strict");
const test = require("node:test");
const { GAMES } = require("./games");

const valorant = GAMES[21640];
const overwatch = GAMES[10844];
const deadlock = GAMES[24482];

test("death opens the overlay in every game", () => {
  for (const game of Object.values(GAMES)) {
    assert.equal(game.toDowntimeEvent({ key: "death", value: "3" }), "death", game.name);
  }
});

test("the end of the match hides it in every game", () => {
  for (const game of Object.values(GAMES)) {
    assert.equal(game.toDowntimeEvent({ key: "match_end", value: null }), "respawn", game.name);
  }
});

test("counters are not events", () => {
  for (const game of Object.values(GAMES)) {
    assert.equal(game.toDowntimeEvent({ key: "deaths", value: "3" }), null, game.name);
  }
});

test("Valorant hides at the next buy phase, not during combat", () => {
  assert.equal(valorant.toDowntimeEvent({ key: "round_phase", value: "shopping" }), "respawn");
  assert.equal(valorant.toDowntimeEvent({ key: "round_phase", value: '"shopping"' }), "respawn");
  assert.equal(valorant.toDowntimeEvent({ key: "round_phase", value: "game_end" }), "respawn");
  assert.equal(valorant.toDowntimeEvent({ key: "round_phase", value: "combat" }), null);
  assert.equal(valorant.toDowntimeEvent({ key: "round_phase", value: "end" }), null);
});

test("Overwatch hides on respawn, revive and a new round", () => {
  for (const key of ["respawn", "revive", "round_start"]) {
    assert.equal(overwatch.toDowntimeEvent({ key, value: null }), "respawn", key);
  }
});

test("Deadlock ignores events it has no use for", () => {
  assert.equal(deadlock.toDowntimeEvent({ key: "kill", value: null }), null);
  assert.equal(deadlock.toDowntimeEvent({ key: "game_paused", value: null }), null);
});
