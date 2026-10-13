/* tslint:disable */
/* eslint-disable */

/**
 * Format that each sample has. Usually, this corresponds to the sampling
 * depth of the audio source. For example, 16 bit quantized samples can be
 * encoded in `i16` or `u16`. Note that the quantized sampling depth is not
 * directly visible for formats where [`is_float`] is true.
 *
 * Also note that the backend must support the encoding of the quantized
 * samples in the given format, as there is no generic transformation from one
 * format into the other done inside the frontend-library code. You can query
 * the supported formats by using [`supported_input_configs`].
 *
 * A good rule of thumb is to use [`SampleFormat::I16`] as this covers typical
 * music (WAV, MP3) as well as typical audio input devices on most platforms,
 *
 * [`is_float`]: SampleFormat::is_float
 * [`supported_input_configs`]: crate::traits::DeviceTrait::supported_input_configs
 */
export enum SampleFormat {
    /**
     * `i8` with a valid range of `i8::MIN..=i8::MAX` with `0` being the origin.
     */
    I8 = 0,
    /**
     * `i16` with a valid range of `i16::MIN..=i16::MAX` with `0` being the origin.
     */
    I16 = 1,
    /**
     * `I24` with a valid range of `-(1 << 23)..=((1 << 23) - 1)` with `0` being the origin.
     *
     * This format uses 4 bytes of storage but only 24 bits are significant.
     */
    I24 = 2,
    /**
     * `i32` with a valid range of `i32::MIN..=i32::MAX` with `0` being the origin.
     */
    I32 = 3,
    /**
     * `i64` with a valid range of `i64::MIN..=i64::MAX` with `0` being the origin.
     */
    I64 = 4,
    /**
     * `u8` with a valid range of `u8::MIN..=u8::MAX` with `1 << 7 == 128` being the origin.
     */
    U8 = 5,
    /**
     * `u16` with a valid range of `u16::MIN..=u16::MAX` with `1 << 15 == 32768` being the origin.
     */
    U16 = 6,
    /**
     * `U24` with a valid range of `0..=((1 << 24) - 1)` with `1 << 23 == 8388608` being the origin.
     *
     * This format uses 4 bytes of storage but only 24 bits are significant.
     */
    U24 = 7,
    /**
     * `u32` with a valid range of `u32::MIN..=u32::MAX` with `1 << 31` being the origin.
     */
    U32 = 8,
    /**
     * `U48` with a valid range of '0..(1 << 48)' with `1 << 47` being the origin
     * `u64` with a valid range of `u64::MIN..=u64::MAX` with `1 << 63` being the origin.
     */
    U64 = 9,
    /**
     * `f32` with a valid range of `-1.0..=1.0` with `0.0` being the origin.
     */
    F32 = 10,
    /**
     * `f64` with a valid range of `-1.0..=1.0` with `0.0` being the origin.
     */
    F64 = 11,
    /**
     * DSD 1-bit stream in u8 container (8 bits = 8 DSD samples) with 0x69 being the silence byte pattern.
     */
    DsdU8 = 12,
    /**
     * DSD 1-bit stream in u16 container (16 bits = 16 DSD samples) with 0x69 being the silence byte pattern.
     */
    DsdU16 = 13,
    /**
     * DSD 1-bit stream in u32 container (32 bits = 32 DSD samples) with 0x69 being the silence byte pattern.
     */
    DsdU32 = 14,
}

/**
 * The set of parameters used to describe how to open a stream.
 *
 * The sample format is omitted in favour of using a sample type.
 *
 * See also [`BufferSize`] for details on buffer size behavior and latency considerations.
 */
export class StreamConfig {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    get buffer_size(): number | undefined;
    set buffer_size(value: number | null | undefined);
    channels: number;
    sample_rate: number;
}

/**
 * Browser-side renderer and application interop exposed to the React host.
 */
export class WebBridge {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Returns the selected GPU backend for diagnostics.
     */
    backend(): string;
    /**
     * Dispatches an application or inspector command.
     */
    command(command: string, payload: any): any;
    /**
     * Advances and presents one frame, returning UI state.
     */
    frame(input: any): any;
    /**
     * Attaches a WebGPU canvas to a viewport; WebGL hosts use asynchronous readback.
     */
    mount_viewport(id: string, canvas: HTMLCanvasElement): number | undefined;
    /**
     * Returns current UI state without advancing the simulation.
     */
    state(): any;
    /**
     * Detaches a previously mounted viewport canvas.
     */
    unmount_viewport(key: number): void;
    /**
     * Reads an offscreen viewport, returning `undefined` while readback is pending.
     */
    viewport(id: string): any;
    /**
     * Resolves when previously submitted GPU work completes.
     *
     * Browser hosts await this between frames to bound the submission queue on slow GPUs.
     */
    wait_for_gpu(): Promise<any>;
}

export function appDir(name: string): string;

export function appendText(path: string, contents: string): void;

export function attachFS(channel: any, filesystem: any): void;

export function configure(configuration: any): Promise<void>;

export function configureFileSystem(filesystem: any, configuration: any): void;

export function configureSingle(configuration: any): Promise<void>;

export function configureSingleSync(configuration: any): void;

export function configureSync(configuration: any): void;

export function constants(): any;

export function copyFile(from: string, to: string): Promise<void>;

export function createDir(path: string): void;

export function createDirAll(path: string): void;

export function defaultFs(): any;

export function detachFS(channel: any, filesystem: any): void;

export function exists(path: string): Promise<boolean>;

export function fs(): any;

export function mount(mount_point: string, filesystem: any): void;

export function mounts(): any;

export function promises(): any;

export function readDir(path: string): string[];

export function readFile(path: string): Promise<Uint8Array>;

export function readText(path: string): string;

export function removeFile(path: string): void;

export function rename(from: string, to: string): Promise<void>;

export function renameSync(from: string, to: string): void;

export function resolveMountConfig(configuration: any): Promise<any>;

export function resolveMountConfigSync(configuration: any): any;

export function resolveRemoteMount(channel: any, configuration: any): Promise<any>;

export function sync(): Promise<void>;

export function umount(mount_point: string): void;

export function version(): any;

export function vfs(): any;

export function waitOnline(worker: any): Promise<void>;

export function writeFile(path: string, contents: Uint8Array): Promise<void>;

export function writeText(path: string, contents: string): void;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly main: (a: number, b: number) => number;
    readonly __wbg_get_streamconfig_buffer_size: (a: number) => number;
    readonly __wbg_get_streamconfig_channels: (a: number) => number;
    readonly __wbg_get_streamconfig_sample_rate: (a: number) => number;
    readonly __wbg_set_streamconfig_buffer_size: (a: number, b: number) => void;
    readonly __wbg_set_streamconfig_channels: (a: number, b: number) => void;
    readonly __wbg_set_streamconfig_sample_rate: (a: number, b: number) => void;
    readonly __wbg_streamconfig_free: (a: number, b: number) => void;
    readonly __wbg_webbridge_free: (a: number, b: number) => void;
    readonly appDir: (a: number, b: number) => [number, number, number, number];
    readonly appendText: (a: number, b: number, c: number, d: number) => [number, number];
    readonly attachFS: (a: any, b: any) => [number, number];
    readonly configure: (a: any) => any;
    readonly configureFileSystem: (a: any, b: any) => [number, number];
    readonly configureSingle: (a: any) => any;
    readonly configureSingleSync: (a: any) => [number, number];
    readonly configureSync: (a: any) => [number, number];
    readonly constants: () => any;
    readonly copyFile: (a: number, b: number, c: number, d: number) => any;
    readonly createDir: (a: number, b: number) => [number, number];
    readonly createDirAll: (a: number, b: number) => [number, number];
    readonly defaultFs: () => any;
    readonly detachFS: (a: any, b: any) => [number, number];
    readonly exists: (a: number, b: number) => any;
    readonly fs: () => any;
    readonly mount: (a: number, b: number, c: any) => [number, number];
    readonly mounts: () => any;
    readonly promises: () => any;
    readonly readDir: (a: number, b: number) => [number, number, number, number];
    readonly readFile: (a: number, b: number) => any;
    readonly readText: (a: number, b: number) => [number, number, number, number];
    readonly removeFile: (a: number, b: number) => [number, number];
    readonly rename: (a: number, b: number, c: number, d: number) => any;
    readonly renameSync: (a: number, b: number, c: number, d: number) => [number, number];
    readonly resolveMountConfig: (a: any) => any;
    readonly resolveMountConfigSync: (a: any) => [number, number, number];
    readonly resolveRemoteMount: (a: any, b: any) => any;
    readonly sync: () => any;
    readonly umount: (a: number, b: number) => [number, number];
    readonly version: () => any;
    readonly vfs: () => any;
    readonly waitOnline: (a: any) => any;
    readonly webbridge_backend: (a: number) => [number, number];
    readonly webbridge_command: (a: number, b: number, c: number, d: any) => [number, number, number];
    readonly webbridge_frame: (a: number, b: any) => [number, number, number];
    readonly webbridge_mount_viewport: (a: number, b: number, c: number, d: any) => [number, number, number];
    readonly webbridge_state: (a: number) => [number, number, number];
    readonly webbridge_unmount_viewport: (a: number, b: number) => void;
    readonly webbridge_viewport: (a: number, b: number, c: number) => [number, number, number];
    readonly webbridge_wait_for_gpu: (a: number) => any;
    readonly writeFile: (a: number, b: number, c: number, d: number) => any;
    readonly writeText: (a: number, b: number, c: number, d: number) => [number, number];
    readonly wasm_bindgen_8582d61557fb93d3___convert__closures_____invoke___js_sys_e46af6bc04ed6125___Function_fn_wasm_bindgen_8582d61557fb93d3___JsValue_____wasm_bindgen_8582d61557fb93d3___sys__Undefined___js_sys_e46af6bc04ed6125___Function_fn_wasm_bindgen_8582d61557fb93d3___JsValue_____wasm_bindgen_8582d61557fb93d3___sys__Undefined_______true_: (a: number, b: number, c: any, d: any) => void;
    readonly wasm_bindgen_8582d61557fb93d3___convert__closures_____invoke___wasm_bindgen_8582d61557fb93d3___JsValue__core_58b45624238b1c28___result__Result_____wasm_bindgen_8582d61557fb93d3___JsError___true_: (a: number, b: number, c: any) => [number, number];
    readonly wasm_bindgen_8582d61557fb93d3___convert__closures_____invoke___wasm_bindgen_8582d61557fb93d3___sys__JsNullable_wgpu_f3c6ab705f305eb1___backend__webgpu__webgpu_sys__gen_GpuError__GpuError___core_58b45624238b1c28___result__Result_____wasm_bindgen_8582d61557fb93d3___JsError___true_: (a: number, b: number, c: any) => [number, number];
    readonly wasm_bindgen_8582d61557fb93d3___convert__closures_____invoke___wasm_bindgen_8582d61557fb93d3___sys__JsNullable_wgpu_f3c6ab705f305eb1___backend__webgpu__webgpu_sys__gen_GpuError__GpuError___core_58b45624238b1c28___result__Result_____wasm_bindgen_8582d61557fb93d3___JsError___true__52: (a: number, b: number, c: any) => [number, number];
    readonly wasm_bindgen_8582d61557fb93d3___convert__closures_____invoke___wasm_bindgen_8582d61557fb93d3___sys__JsNullable_wgpu_f3c6ab705f305eb1___backend__webgpu__webgpu_sys__gen_GpuError__GpuError___core_58b45624238b1c28___result__Result_____wasm_bindgen_8582d61557fb93d3___JsError___true__53: (a: number, b: number, c: any) => [number, number];
    readonly wasm_bindgen_8582d61557fb93d3___convert__closures_____invoke_______true_: (a: number, b: number) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_destroy_closure: (a: number, b: number) => void;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __externref_drop_slice: (a: number, b: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
