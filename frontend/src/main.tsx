import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
// DM Mono（数字・タイムスタンプ用）をローカル同梱（CDN 非依存＝オフラインで動く）。
import "@fontsource/dm-mono/400.css";
import "@fontsource/dm-mono/500.css";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
