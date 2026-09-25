export function orbitPosition(angle, radius) {
  return [Math.cos(angle) * radius, 0.35, Math.sin(angle) * radius];
}

/** @implements {TerrariumScript} */
export default class Ring {
  onLoad() {
    this.parts = Array.from({ length: 8 }, (_, index) => {
      const angle = (index / 8) * Math.PI * 2;
      return new Instance("Part", {
        name: `Ring${index}`,
        position: orbitPosition(angle, 3.5),
        size: [0.35, 0.35, 0.35],
        color: [0.1, 0.8 - index * 0.05, 0.55],
        anchored: true,
      });
    });
  }

  onUnload() {
    for (const part of this.parts) part.destroy();
  }

  /** @param {number} deltaTime Frame time in seconds. */
  onUpdate(deltaTime) {
    const rotation = time * 0.8;
    for (let index = 0; index < this.parts.length; index += 1) {
      const angle = rotation + (index / this.parts.length) * Math.PI * 2;
      const [x, y, z] = orbitPosition(angle, 3.5);
      this.parts[index].setPosition([x, y + Math.sin(time * 2 + index) * 0.15, z]);
    }
  }
}
