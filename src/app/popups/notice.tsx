import React from "react";
import ReactDOM from "react-dom/client";
import { NoticeCard } from "./NoticeCard";
import "@/shared/ui/tokens.css";
import "./popups.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <NoticeCard />
  </React.StrictMode>,
);
