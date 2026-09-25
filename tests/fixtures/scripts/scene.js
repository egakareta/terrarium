import { name } from "./helpers/name.js";

export default class Scene {
  onLoad() {
    new Instance("Part", { name });
  }
}
