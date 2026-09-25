/** A three-component vector represented as `[x, y, z]`. */
type Vector3 = readonly [number, number, number];

/** An RGB color represented as `[red, green, blue]`, with each component from 0 to 1. */
type Color3 = readonly [number, number, number];

/** Primitive geometry supported by a `Part` instance. */
type PartShape = "block" | "ball" | "cylinder" | "wedge" | "cornerWedge";

/** Options used when creating a `Part` with `new Instance("Part", options)`. */
interface PartOptions {
  /** Display name shown by the scene object. */
  name?: string;
  /** Primitive geometry used to render the part. Defaults to `"block"`. */
  shape?: PartShape;
  /** World-space position of the part's pivot. Defaults to `[0, 0, 0]`. */
  position?: Vector3;
  /** World-space size of the part. Defaults to `[1, 1, 1]`. */
  size?: Vector3;
  /** RGB tint applied to the part. Defaults to `[1, 1, 1]`. */
  color?: Color3;
  /** Transparency from `0` (opaque) to `1` (invisible). Defaults to `0`. */
  transparency?: number;
  /** Whether physics treats the part as immovable. Defaults to `true`. */
  anchored?: boolean;
  /** Whether the part participates in collisions. Defaults to `true`. */
  canCollide?: boolean;
}

/**
 * A scene object created from JavaScript.
 *
 * Instances queue scene changes for Terrarium to apply at the end of the
 * current script phase. Mutating a destroyed instance is ignored.
 */
declare class Instance {
  // TODO
  /** Creates a supported scene object. The current runtime supports `"Part"`. */
  constructor(type: "Part", options?: PartOptions);

  /** Removes this object from the scene. Calling this more than once is safe. */
  destroy(): void;

  /** Sets the world-space position of the object's pivot. */
  setPosition(position: Vector3): this;

  /** Sets XYZ Euler orientation in degrees. */
  setOrientation(orientation: Vector3): this;

  /** Sets the object's world-space size. */
  setSize(size: Vector3): this;

  /** Sets the object's RGB tint. */
  setColor(color: Color3): this;

  /** Sets the object's display name. */
  setName(name: string): this;

  /** Sets transparency from `0` (opaque) to `1` (invisible). */
  setTransparency(transparency: number): this;

  /** Sets whether physics treats the object as immovable. */
  setAnchored(anchored: boolean): this;

  /** Sets whether the object participates in collisions. */
  setCanCollide(canCollide: boolean): this;
}

/**
 * Lifecycle implemented by the default-exported class of a script module.
 *
 * JSDoc constructor parameter types resolve to imported or local class bindings. Dependencies
 * are shared across loaded script graphs. Hooks run dependency-first, with `onBeforeReload`
 * and `onUnload` running in reverse order when the last graph releases them.
 */
interface Component {
  /** Runs once after the module class is constructed. */
  onInit?(): void | Promise<void>;
  /** Runs once after initialization and before the first update. */
  onLoad?(): void | Promise<void>;
  /** Runs once per Terrarium update with the frame delta in seconds. */
  onUpdate?(deltaTime: number): void;
  /** Runs before a hot reload replaces this module. */
  onBeforeReload?(): void | Promise<void>;
  /** Runs when the module is unloaded or replaced. */
  onUnload?(): void | Promise<void>;
  /** Runs on the replacement module after a successful hot reload. */
  onReload?(): void | Promise<void>;
}

/** Elapsed Terrarium runtime time in seconds. */
declare const time: number;

/** Writes a message to the Terrarium JavaScript log. */
declare function log(message: string): void;
