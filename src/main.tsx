import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import ToastStack from "./components/ToastStack";

// The toast window loads the same bundle with ?toast=1 and renders only
// the notification stack (borderless, transparent, always-on-top).
const isToastWindow = new URLSearchParams(window.location.search).has("toast");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {isToastWindow ? <ToastStack /> : <App />}
  </React.StrictMode>,
);
