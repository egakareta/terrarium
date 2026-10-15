
function canResolveZenFs() {
    try {
        if (typeof import.meta.resolve === "function") {
            import.meta.resolve("@zenfs/core");
            return true;
        }
    } catch (_) {
        return false;
    }

    try {
        return [...document.querySelectorAll('script[type="importmap"]')]
            .some((script) => Object.hasOwn(JSON.parse(script.textContent || "{}").imports || {}, "@zenfs/core"));
    } catch (_) {
        return false;
    }
}

let zenfsModule = null;
const zenfsModulePromise = canResolveZenFs()
    ? import("@zenfs/core").then((module) => (zenfsModule = module, module)).catch(() => null)
    : Promise.resolve(null);

function requireZenFs() {
    if (!zenfsModule) {
        throw new Error("ZenFS is not available or has not finished loading");
    }
    return zenfsModule;
}

export function zenfs_is_configured() {
    if (!zenfsModule) return false;
    const mounts = zenfsModule.mounts;
    return mounts instanceof Map ? mounts.size > 0 : Object.keys(mounts || {}).length > 0;
}

export function zenfs_get_export(name) {
    return zenfsModule ? zenfsModule[name] : undefined;
}

export function zenfs_call_sync(name, args) {
    const fn = requireZenFs()[name];
    if (typeof fn !== "function") throw new TypeError(`ZenFS export ${name} is not a function`);
    return fn(...args);
}

export async function zenfs_call_async(name, args) {
    let module = zenfsModule || await zenfsModulePromise;
    if (!module && canResolveZenFs()) {
        module = await import("@zenfs/core").then((loaded) => (zenfsModule = loaded, loaded)).catch(() => null);
    }
    if (!module) throw new Error("ZenFS is not available; enable the webfs feature to use its fallback");
    const fn = module[name];
    if (typeof fn !== "function") throw new TypeError(`ZenFS export ${name} is not a function`);
    return fn(...args);
}
