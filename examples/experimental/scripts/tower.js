export const towerLevels = 5;

/** @implements {TerrariumScript} */
export default class Tower {
  onLoad() {
    this.blocks = Array.from({ length: towerLevels }, (_, index) => {
      return new Instance("Part", {
        name: `Tower${index}`,
        position: [2.75, 0.35 + index * 0.7, 0],
        size: [0.9, 0.55, 0.9],
        color: [0.3 + index * 0.1, 0.25, 0.8 - index * 0.08],
        anchored: true,
      });
    });
  }

  onUnload() {
    for (const block of this.blocks) block.destroy();
  }

  /** @param {number} deltaTime Frame time in seconds. */
  onUpdate(deltaTime) {
    for (const [index, block] of this.blocks.entries()) {
      block.setOrientation([0, time * 25 + index * 12, 0]);
    }
  }
}
