const { app } = require("electron");
const fs = require("node:fs");
const net = require("node:net");
const path = require("node:path");
const { GAMES } = require("./games");

const BRIDGE_PORT = 47631; // BRIDGE_PORT in src-tauri/src/game_bridge.rs
const LOG_FILE = path.join(__dirname, "events.log");

function log(kind, details) {
  const line = JSON.stringify({ time: new Date().toISOString(), kind, ...details });
  console.log(line);
  fs.appendFileSync(LOG_FILE, line + "\n");
}

function sendToDowntime(event) {
  const socket = net.connect(BRIDGE_PORT, "127.0.0.1", () => socket.end(event + "\n"));
  socket.on("error", (error) => log("bridge-error", { event, message: error.message }));
}

function onGameData(kind, gameId, data) {
  log(kind, { gameId, data });
  const event = GAMES[gameId]?.toDowntimeEvent(data);
  if (event) {
    log("forwarded", { event });
    sendToDowntime(event);
  }
}

function startGameEvents() {
  const gep = app.overwolf.packages.gep;

  gep.on("game-detected", async (event, gameId, name) => {
    log("game-detected", { gameId, name });
    const game = GAMES[gameId];
    if (!game) return;
    event.enable();
    try {
      await gep.setRequiredFeatures(gameId, game.features);
      log("features-set", { features: game.features });
    } catch (error) {
      log("features-error", { message: String(error) });
    }
  });
  gep.on("new-game-event", (_event, gameId, data) => onGameData("game-event", gameId, data));
  gep.on("new-info-update", (_event, gameId, data) => onGameData("info-update", gameId, data));
  gep.on("elevated-privileges-required", (_event, gameId, name) => {
    log("elevated-privileges-required", { gameId, name });
  });
  gep.on("error", (_event, gameId, error) => log("gep-error", { gameId, error }));
  gep.on("game-exit", (_event, gameId, name) => log("game-exit", { gameId, name }));
}

app.overwolf.packages.on("ready", (_event, name, version) => {
  log("package-ready", { name, version });
  if (name === "gep") startGameEvents();
});
app.overwolf.packages.on("failed-to-initialize", (_event, name) => {
  log("package-failed", { name });
});

// The helper has no windows, so it must not quit when none are open.
app.on("window-all-closed", () => {});
app.whenReady().then(() => log("started", { devKey: Boolean(process.env.OW_DEV_KEY) }));
