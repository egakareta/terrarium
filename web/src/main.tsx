import { createRoot } from "react-dom/client";
import { TerrariumHost } from "./host";
import { isTauri } from "@tauri-apps/api/core";
import { exampleFromLocation } from "./routing";

const query = new URLSearchParams(location.search);
const gallery = !isTauri() && !exampleFromLocation() && !query.has("widget");
createRoot(document.getElementById("root")!).render(<TerrariumHost gallery={gallery} />);
