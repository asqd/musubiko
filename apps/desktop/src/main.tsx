import React from "react";
import ReactDOM from "react-dom/client";
import "@fontsource/ibm-plex-sans/400.css";
import "@fontsource/ibm-plex-sans/500.css";
import "@fontsource/ibm-plex-sans/600.css";
import "@fontsource/ibm-plex-sans/700.css";
import "@fontsource/ibm-plex-mono/400.css";
import "@fontsource/ibm-plex-mono/500.css";
import "./styles.css";
import App from "./App";
import { Crash, logScriptErrors } from "./components/ErrorBanner";
import { isMac } from "./platform";

logScriptErrors();
// The traffic lights sit over the top bar only on a Mac.
if (isMac) document.documentElement.classList.add("mac");

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Crash>
      <App />
    </Crash>
  </React.StrictMode>,
);
