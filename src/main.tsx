import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import HomeView from "./HomeView";
import OverlayView from "./OverlayView";
import TitleBar from "./components/TitleBar";
import "./index.css";

const root = document.getElementById("root");
if (!root) throw new Error("Missing #root element");

createRoot(root).render(
  <StrictMode>
    {getCurrentWindow().label === "overlay" ? <OverlayView /> : (
      <div className="flex h-dvh flex-col bg-paper">
        <TitleBar />
        <div className="min-h-0 flex-1"><HomeView /></div>
      </div>
    )}
  </StrictMode>,
);
