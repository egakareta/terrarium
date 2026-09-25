import { orbitPosition } from "./ring.js";
import { towerLevels } from "./tower.js";
// oxlint-disable-next-line no-unused-vars
import Ring from "./ring.js";

/** @implements {Component} */
export default class Scene {
  /** @param {Ring} ring */
  constructor(ring) {
    this.ring = ring;
  }

  onInit() {
    log("scene initialized");
  }

  onLoad() {
    const platformWidth = towerLevels * 0.7;
    this.orb = new Instance("Part", {
      name: "Orb",
      shape: "ball",
      position: [0, 1.5, 0],
      size: [0.8, 0.8, 0.8],
      color: [1.0, 0.35, 0.08],
      canCollide: false,
    });

    this.platform = new Instance("Part", {
      name: "Platform",
      shape: "block",
      position: [0, 0, 0],
      size: [platformWidth, 0.3, platformWidth],
      color: [0.15, 0.55, 0.75],
      anchored: true,
    });
  }

  onUnload() {
    this.orb.destroy();
    this.platform.destroy();
  }

  /** @param {number} deltaTime Frame time in seconds. */
  onUpdate(deltaTime) {
    const [x, , z] = orbitPosition(time * 1.5, 1.7);
    this.orb.setPosition([x, 1.5 + Math.sin(time * 2.0) * 0.35, z]);
    this.platform.setOrientation([0, time * 35.0, 0]);
  }
}
