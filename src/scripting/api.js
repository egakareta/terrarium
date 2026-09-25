globalThis.__terrarium_construct = (constructor, args) => Reflect.construct(constructor, args);

globalThis.Instance = ((
  create,
  destroy,
  setPosition,
  setOrientation,
  setSize,
  setColor,
  setName,
  setTransparency,
  setAnchored,
  setCanCollide,
  isActive,
) =>
  class Instance {
    constructor(type, options) {
      this._handle = create(type, options);
      this._destroyedFlag = false;
    }

    get _destroyed() {
      if (this._destroyedFlag) this._destroyedFlag = !isActive(this._handle);
      return this._destroyedFlag;
    }

    destroy() {
      if (!this._destroyed) {
        destroy(this._handle);
        this._destroyedFlag = true;
      }
    }

    setPosition(values) {
      if (!this._destroyed) setPosition(this._handle, values);
      return this;
    }

    setOrientation(values) {
      if (!this._destroyed) setOrientation(this._handle, values);
      return this;
    }

    setSize(values) {
      if (!this._destroyed) setSize(this._handle, values);
      return this;
    }

    setColor(values) {
      if (!this._destroyed) setColor(this._handle, values);
      return this;
    }

    setName(name) {
      if (!this._destroyed) setName(this._handle, name);
      return this;
    }

    setTransparency(value) {
      if (!this._destroyed) setTransparency(this._handle, value);
      return this;
    }

    setAnchored(value) {
      if (!this._destroyed) setAnchored(this._handle, value);
      return this;
    }

    setCanCollide(value) {
      if (!this._destroyed) setCanCollide(this._handle, value);
      return this;
    }
  })(
  __terrarium_create_instance,
  __terrarium_destroy_instance,
  __terrarium_set_position,
  __terrarium_set_orientation,
  __terrarium_set_size,
  __terrarium_set_color,
  __terrarium_set_name,
  __terrarium_set_transparency,
  __terrarium_set_anchored,
  __terrarium_set_can_collide,
  __terrarium_is_active,
);

delete globalThis.__terrarium_create_instance;
delete globalThis.__terrarium_destroy_instance;
delete globalThis.__terrarium_set_position;
delete globalThis.__terrarium_set_orientation;
delete globalThis.__terrarium_set_size;
delete globalThis.__terrarium_set_color;
delete globalThis.__terrarium_set_name;
delete globalThis.__terrarium_set_transparency;
delete globalThis.__terrarium_set_anchored;
delete globalThis.__terrarium_set_can_collide;
delete globalThis.__terrarium_is_active;
