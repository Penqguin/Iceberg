(() => {
  var __defProp = Object.defineProperty;
  var __name = (target, value) => __defProp(target, "name", { value, configurable: true });
  var __export = (target, all) => {
    for (var name in all)
      __defProp(target, name, { get: all[name], enumerable: true });
  };

  // wrangler-module-CompiledWasm:./9521c4473d8da76029da8f1dbeb7a4c422565340-index_bg.wasm
  var c4473d8da76029da8f1dbeb7a4c422565340_index_bg_exports = {};
  __export(c4473d8da76029da8f1dbeb7a4c422565340_index_bg_exports, {
    default: () => c4473d8da76029da8f1dbeb7a4c422565340_index_bg_default
  });
  var c4473d8da76029da8f1dbeb7a4c422565340_index_bg_default = __9521c4473d8da76029da8f1dbeb7a4c422565340_index_bg_wasm;

  // build/index_bg.js
  var ContainerStartupOptions = class {
    static {
      __name(this, "ContainerStartupOptions");
    }
    __destroy_into_raw() {
      const ptr = this.__wbg_ptr;
      this.__wbg_ptr = 0;
      ContainerStartupOptionsFinalization.unregister(this);
      return ptr;
    }
    free() {
      const ptr = this.__destroy_into_raw();
      __wbg_call_guard();
      wasm.__wbg_containerstartupoptions_free(ptr, 0);
    }
    /**
     * @returns {boolean | undefined}
     */
    get enableInternet() {
      if (this.__wbg_inst !== void 0 && this.__wbg_inst !== __wbg_instance_id) {
        throw new Error("Invalid stale object from previous Wasm instance");
      }
      let ret;
      __wbg_call_guard();
      ret = wasm.__wbg_get_containerstartupoptions_enableInternet(this.__wbg_ptr);
      return ret === 16777215 ? void 0 : ret !== 0;
    }
    /**
     * @returns {string[]}
     */
    get entrypoint() {
      if (this.__wbg_inst !== void 0 && this.__wbg_inst !== __wbg_instance_id) {
        throw new Error("Invalid stale object from previous Wasm instance");
      }
      let ret;
      __wbg_call_guard();
      ret = wasm.__wbg_get_containerstartupoptions_entrypoint(this.__wbg_ptr);
      var v1 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
      wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
      return v1;
    }
    /**
     * @returns {Map<any, any>}
     */
    get env() {
      if (this.__wbg_inst !== void 0 && this.__wbg_inst !== __wbg_instance_id) {
        throw new Error("Invalid stale object from previous Wasm instance");
      }
      let ret;
      __wbg_call_guard();
      ret = wasm.__wbg_get_containerstartupoptions_env(this.__wbg_ptr);
      return ret;
    }
    /**
     * @param {boolean | null} [arg0]
     */
    set enableInternet(arg0) {
      if (this.__wbg_inst !== void 0 && this.__wbg_inst !== __wbg_instance_id) {
        throw new Error("Invalid stale object from previous Wasm instance");
      }
      __wbg_call_guard();
      wasm.__wbg_set_containerstartupoptions_enableInternet(this.__wbg_ptr, isLikeNone(arg0) ? 16777215 : arg0 ? 1 : 0);
    }
    /**
     * @param {string[]} arg0
     */
    set entrypoint(arg0) {
      if (this.__wbg_inst !== void 0 && this.__wbg_inst !== __wbg_instance_id) {
        throw new Error("Invalid stale object from previous Wasm instance");
      }
      const ptr0 = passArrayJsValueToWasm0(arg0, wasm.__wbindgen_malloc);
      const len0 = WASM_VECTOR_LEN;
      __wbg_call_guard();
      wasm.__wbg_set_containerstartupoptions_entrypoint(this.__wbg_ptr, ptr0, len0);
    }
    /**
     * @param {Map<any, any>} arg0
     */
    set env(arg0) {
      if (this.__wbg_inst !== void 0 && this.__wbg_inst !== __wbg_instance_id) {
        throw new Error("Invalid stale object from previous Wasm instance");
      }
      __wbg_call_guard();
      wasm.__wbg_set_containerstartupoptions_env(this.__wbg_ptr, arg0);
    }
  };
  if (Symbol.dispose) ContainerStartupOptions.prototype[Symbol.dispose] = ContainerStartupOptions.prototype.free;
  var IntoUnderlyingByteSource = class {
    static {
      __name(this, "IntoUnderlyingByteSource");
    }
    __destroy_into_raw() {
      const ptr = this.__wbg_ptr;
      this.__wbg_ptr = 0;
      IntoUnderlyingByteSourceFinalization.unregister(this);
      return ptr;
    }
    free() {
      const ptr = this.__destroy_into_raw();
      __wbg_call_guard();
      wasm.__wbg_intounderlyingbytesource_free(ptr, 0);
    }
    /**
     * @returns {number}
     */
    get autoAllocateChunkSize() {
      if (this.__wbg_inst !== void 0 && this.__wbg_inst !== __wbg_instance_id) {
        throw new Error("Invalid stale object from previous Wasm instance");
      }
      let ret;
      __wbg_call_guard();
      ret = wasm.intounderlyingbytesource_autoAllocateChunkSize(this.__wbg_ptr);
      return ret >>> 0;
    }
    cancel() {
      if (this.__wbg_inst !== void 0 && this.__wbg_inst !== __wbg_instance_id) {
        throw new Error("Invalid stale object from previous Wasm instance");
      }
      const ptr = this.__destroy_into_raw();
      __wbg_call_guard();
      wasm.intounderlyingbytesource_cancel(ptr);
    }
    /**
     * @param {ReadableByteStreamController} controller
     * @returns {Promise<any>}
     */
    pull(controller) {
      if (this.__wbg_inst !== void 0 && this.__wbg_inst !== __wbg_instance_id) {
        throw new Error("Invalid stale object from previous Wasm instance");
      }
      let ret;
      __wbg_call_guard();
      ret = wasm.intounderlyingbytesource_pull(this.__wbg_ptr, controller);
      return ret;
    }
    /**
     * @param {ReadableByteStreamController} controller
     */
    start(controller) {
      if (this.__wbg_inst !== void 0 && this.__wbg_inst !== __wbg_instance_id) {
        throw new Error("Invalid stale object from previous Wasm instance");
      }
      __wbg_call_guard();
      wasm.intounderlyingbytesource_start(this.__wbg_ptr, controller);
    }
    /**
     * @returns {ReadableStreamType}
     */
    get type() {
      if (this.__wbg_inst !== void 0 && this.__wbg_inst !== __wbg_instance_id) {
        throw new Error("Invalid stale object from previous Wasm instance");
      }
      let ret;
      __wbg_call_guard();
      ret = wasm.intounderlyingbytesource_type(this.__wbg_ptr);
      return __wbindgen_enum_ReadableStreamType[ret];
    }
  };
  if (Symbol.dispose) IntoUnderlyingByteSource.prototype[Symbol.dispose] = IntoUnderlyingByteSource.prototype.free;
  var IntoUnderlyingSink = class {
    static {
      __name(this, "IntoUnderlyingSink");
    }
    __destroy_into_raw() {
      const ptr = this.__wbg_ptr;
      this.__wbg_ptr = 0;
      IntoUnderlyingSinkFinalization.unregister(this);
      return ptr;
    }
    free() {
      const ptr = this.__destroy_into_raw();
      __wbg_call_guard();
      wasm.__wbg_intounderlyingsink_free(ptr, 0);
    }
    /**
     * @param {any} reason
     * @returns {Promise<any>}
     */
    abort(reason) {
      if (this.__wbg_inst !== void 0 && this.__wbg_inst !== __wbg_instance_id) {
        throw new Error("Invalid stale object from previous Wasm instance");
      }
      const ptr = this.__destroy_into_raw();
      let ret;
      __wbg_call_guard();
      ret = wasm.intounderlyingsink_abort(ptr, reason);
      return ret;
    }
    /**
     * @returns {Promise<any>}
     */
    close() {
      if (this.__wbg_inst !== void 0 && this.__wbg_inst !== __wbg_instance_id) {
        throw new Error("Invalid stale object from previous Wasm instance");
      }
      const ptr = this.__destroy_into_raw();
      let ret;
      __wbg_call_guard();
      ret = wasm.intounderlyingsink_close(ptr);
      return ret;
    }
    /**
     * @param {any} chunk
     * @returns {Promise<any>}
     */
    write(chunk) {
      if (this.__wbg_inst !== void 0 && this.__wbg_inst !== __wbg_instance_id) {
        throw new Error("Invalid stale object from previous Wasm instance");
      }
      let ret;
      __wbg_call_guard();
      ret = wasm.intounderlyingsink_write(this.__wbg_ptr, chunk);
      return ret;
    }
  };
  if (Symbol.dispose) IntoUnderlyingSink.prototype[Symbol.dispose] = IntoUnderlyingSink.prototype.free;
  var IntoUnderlyingSource = class {
    static {
      __name(this, "IntoUnderlyingSource");
    }
    __destroy_into_raw() {
      const ptr = this.__wbg_ptr;
      this.__wbg_ptr = 0;
      IntoUnderlyingSourceFinalization.unregister(this);
      return ptr;
    }
    free() {
      const ptr = this.__destroy_into_raw();
      __wbg_call_guard();
      wasm.__wbg_intounderlyingsource_free(ptr, 0);
    }
    cancel() {
      if (this.__wbg_inst !== void 0 && this.__wbg_inst !== __wbg_instance_id) {
        throw new Error("Invalid stale object from previous Wasm instance");
      }
      const ptr = this.__destroy_into_raw();
      __wbg_call_guard();
      wasm.intounderlyingsource_cancel(ptr);
    }
    /**
     * @param {ReadableStreamDefaultController} controller
     * @returns {Promise<any>}
     */
    pull(controller) {
      if (this.__wbg_inst !== void 0 && this.__wbg_inst !== __wbg_instance_id) {
        throw new Error("Invalid stale object from previous Wasm instance");
      }
      let ret;
      __wbg_call_guard();
      ret = wasm.intounderlyingsource_pull(this.__wbg_ptr, controller);
      return ret;
    }
  };
  if (Symbol.dispose) IntoUnderlyingSource.prototype[Symbol.dispose] = IntoUnderlyingSource.prototype.free;
  var MinifyConfig = class _MinifyConfig {
    static {
      __name(this, "MinifyConfig");
    }
    static __wrap(ptr) {
      const obj = Object.create(_MinifyConfig.prototype);
      obj.__wbg_ptr = ptr;
      Object.defineProperty(obj, "__wbg_inst", { value: __wbg_instance_id, writable: true });
      MinifyConfigFinalization.register(obj, { ptr, instance: __wbg_instance_id }, obj);
      return obj;
    }
    __destroy_into_raw() {
      const ptr = this.__wbg_ptr;
      this.__wbg_ptr = 0;
      MinifyConfigFinalization.unregister(this);
      return ptr;
    }
    free() {
      const ptr = this.__destroy_into_raw();
      __wbg_call_guard();
      wasm.__wbg_minifyconfig_free(ptr, 0);
    }
    /**
     * @returns {boolean}
     */
    get css() {
      if (this.__wbg_inst !== void 0 && this.__wbg_inst !== __wbg_instance_id) {
        throw new Error("Invalid stale object from previous Wasm instance");
      }
      let ret;
      __wbg_call_guard();
      ret = wasm.__wbg_get_minifyconfig_css(this.__wbg_ptr);
      return ret !== 0;
    }
    /**
     * @returns {boolean}
     */
    get html() {
      if (this.__wbg_inst !== void 0 && this.__wbg_inst !== __wbg_instance_id) {
        throw new Error("Invalid stale object from previous Wasm instance");
      }
      let ret;
      __wbg_call_guard();
      ret = wasm.__wbg_get_minifyconfig_html(this.__wbg_ptr);
      return ret !== 0;
    }
    /**
     * @returns {boolean}
     */
    get js() {
      if (this.__wbg_inst !== void 0 && this.__wbg_inst !== __wbg_instance_id) {
        throw new Error("Invalid stale object from previous Wasm instance");
      }
      let ret;
      __wbg_call_guard();
      ret = wasm.__wbg_get_minifyconfig_js(this.__wbg_ptr);
      return ret !== 0;
    }
    /**
     * @param {boolean} arg0
     */
    set css(arg0) {
      if (this.__wbg_inst !== void 0 && this.__wbg_inst !== __wbg_instance_id) {
        throw new Error("Invalid stale object from previous Wasm instance");
      }
      __wbg_call_guard();
      wasm.__wbg_set_minifyconfig_css(this.__wbg_ptr, arg0);
    }
    /**
     * @param {boolean} arg0
     */
    set html(arg0) {
      if (this.__wbg_inst !== void 0 && this.__wbg_inst !== __wbg_instance_id) {
        throw new Error("Invalid stale object from previous Wasm instance");
      }
      __wbg_call_guard();
      wasm.__wbg_set_minifyconfig_html(this.__wbg_ptr, arg0);
    }
    /**
     * @param {boolean} arg0
     */
    set js(arg0) {
      if (this.__wbg_inst !== void 0 && this.__wbg_inst !== __wbg_instance_id) {
        throw new Error("Invalid stale object from previous Wasm instance");
      }
      __wbg_call_guard();
      wasm.__wbg_set_minifyconfig_js(this.__wbg_ptr, arg0);
    }
  };
  if (Symbol.dispose) MinifyConfig.prototype[Symbol.dispose] = MinifyConfig.prototype.free;
  var R2Range = class {
    static {
      __name(this, "R2Range");
    }
    __destroy_into_raw() {
      const ptr = this.__wbg_ptr;
      this.__wbg_ptr = 0;
      R2RangeFinalization.unregister(this);
      return ptr;
    }
    free() {
      const ptr = this.__destroy_into_raw();
      __wbg_call_guard();
      wasm.__wbg_r2range_free(ptr, 0);
    }
    /**
     * @returns {number | undefined}
     */
    get length() {
      if (this.__wbg_inst !== void 0 && this.__wbg_inst !== __wbg_instance_id) {
        throw new Error("Invalid stale object from previous Wasm instance");
      }
      let ret;
      __wbg_call_guard();
      ret = wasm.__wbg_get_r2range_length(this.__wbg_ptr);
      return ret[0] === 0 ? void 0 : ret[1];
    }
    /**
     * @returns {number | undefined}
     */
    get offset() {
      if (this.__wbg_inst !== void 0 && this.__wbg_inst !== __wbg_instance_id) {
        throw new Error("Invalid stale object from previous Wasm instance");
      }
      let ret;
      __wbg_call_guard();
      ret = wasm.__wbg_get_r2range_offset(this.__wbg_ptr);
      return ret[0] === 0 ? void 0 : ret[1];
    }
    /**
     * @returns {number | undefined}
     */
    get suffix() {
      if (this.__wbg_inst !== void 0 && this.__wbg_inst !== __wbg_instance_id) {
        throw new Error("Invalid stale object from previous Wasm instance");
      }
      let ret;
      __wbg_call_guard();
      ret = wasm.__wbg_get_r2range_suffix(this.__wbg_ptr);
      return ret[0] === 0 ? void 0 : ret[1];
    }
    /**
     * @param {number | null} [arg0]
     */
    set length(arg0) {
      if (this.__wbg_inst !== void 0 && this.__wbg_inst !== __wbg_instance_id) {
        throw new Error("Invalid stale object from previous Wasm instance");
      }
      __wbg_call_guard();
      wasm.__wbg_set_r2range_length(this.__wbg_ptr, !isLikeNone(arg0), isLikeNone(arg0) ? 0 : arg0);
    }
    /**
     * @param {number | null} [arg0]
     */
    set offset(arg0) {
      if (this.__wbg_inst !== void 0 && this.__wbg_inst !== __wbg_instance_id) {
        throw new Error("Invalid stale object from previous Wasm instance");
      }
      __wbg_call_guard();
      wasm.__wbg_set_r2range_offset(this.__wbg_ptr, !isLikeNone(arg0), isLikeNone(arg0) ? 0 : arg0);
    }
    /**
     * @param {number | null} [arg0]
     */
    set suffix(arg0) {
      if (this.__wbg_inst !== void 0 && this.__wbg_inst !== __wbg_instance_id) {
        throw new Error("Invalid stale object from previous Wasm instance");
      }
      __wbg_call_guard();
      wasm.__wbg_set_r2range_suffix(this.__wbg_ptr, !isLikeNone(arg0), isLikeNone(arg0) ? 0 : arg0);
    }
  };
  if (Symbol.dispose) R2Range.prototype[Symbol.dispose] = R2Range.prototype.free;
  function __wbg_reset_state() {
    __wbg_instance_id++;
    cachedDataViewMemory0 = null;
    cachedUint8ArrayMemory0 = null;
    if (typeof numBytesDecoded !== "undefined") numBytesDecoded = 0;
    if (typeof WASM_VECTOR_LEN !== "undefined") WASM_VECTOR_LEN = 0;
    __wbg_reinit_scheduled = false;
    wasmInstance = new WebAssembly.Instance(wasmModule, __wbg_get_imports());
    wasm = wasmInstance.exports;
    wasm.__wbindgen_start();
  }
  __name(__wbg_reset_state, "__wbg_reset_state");
  function __worker_init_state() {
    let ret;
    __wbg_call_guard();
    ret = wasm.__worker_init_state();
    return ret;
  }
  __name(__worker_init_state, "__worker_init_state");
  function fetch(req, env, ctx) {
    let ret;
    __wbg_call_guard();
    ret = wasm.fetch(req, env, ctx);
    return ret;
  }
  __name(fetch, "fetch");
  function init() {
    __wbg_call_guard();
    wasm.init();
  }
  __name(init, "init");
  function __wbg_call_guard() {
    if (__wbg_reinit_scheduled) {
      __wbg_reset_state();
      return;
    }
  }
  __name(__wbg_call_guard, "__wbg_call_guard");
  var __wbindgen_enum_ReadableStreamType = ["bytes"];
  var __wbg_instance_id = 0;
  var ContainerStartupOptionsFinalization = typeof FinalizationRegistry === "undefined" ? { register: /* @__PURE__ */ __name(() => {
  }, "register"), unregister: /* @__PURE__ */ __name(() => {
  }, "unregister") } : new FinalizationRegistry(({ ptr, instance }) => {
    if (instance === __wbg_instance_id) wasm.__wbg_containerstartupoptions_free(ptr, 1);
  });
  var IntoUnderlyingByteSourceFinalization = typeof FinalizationRegistry === "undefined" ? { register: /* @__PURE__ */ __name(() => {
  }, "register"), unregister: /* @__PURE__ */ __name(() => {
  }, "unregister") } : new FinalizationRegistry(({ ptr, instance }) => {
    if (instance === __wbg_instance_id) wasm.__wbg_intounderlyingbytesource_free(ptr, 1);
  });
  var IntoUnderlyingSinkFinalization = typeof FinalizationRegistry === "undefined" ? { register: /* @__PURE__ */ __name(() => {
  }, "register"), unregister: /* @__PURE__ */ __name(() => {
  }, "unregister") } : new FinalizationRegistry(({ ptr, instance }) => {
    if (instance === __wbg_instance_id) wasm.__wbg_intounderlyingsink_free(ptr, 1);
  });
  var IntoUnderlyingSourceFinalization = typeof FinalizationRegistry === "undefined" ? { register: /* @__PURE__ */ __name(() => {
  }, "register"), unregister: /* @__PURE__ */ __name(() => {
  }, "unregister") } : new FinalizationRegistry(({ ptr, instance }) => {
    if (instance === __wbg_instance_id) wasm.__wbg_intounderlyingsource_free(ptr, 1);
  });
  var MinifyConfigFinalization = typeof FinalizationRegistry === "undefined" ? { register: /* @__PURE__ */ __name(() => {
  }, "register"), unregister: /* @__PURE__ */ __name(() => {
  }, "unregister") } : new FinalizationRegistry(({ ptr, instance }) => {
    if (instance === __wbg_instance_id) wasm.__wbg_minifyconfig_free(ptr, 1);
  });
  var R2RangeFinalization = typeof FinalizationRegistry === "undefined" ? { register: /* @__PURE__ */ __name(() => {
  }, "register"), unregister: /* @__PURE__ */ __name(() => {
  }, "unregister") } : new FinalizationRegistry(({ ptr, instance }) => {
    if (instance === __wbg_instance_id) wasm.__wbg_r2range_free(ptr, 1);
  });
  function addToExternrefTable0(obj) {
    const idx = wasm.__externref_table_alloc();
    wasm.__wbindgen_externrefs.set(idx, obj);
    return idx;
  }
  __name(addToExternrefTable0, "addToExternrefTable0");
  var CLOSURE_DTORS = typeof FinalizationRegistry === "undefined" ? { register: /* @__PURE__ */ __name(() => {
  }, "register"), unregister: /* @__PURE__ */ __name(() => {
  }, "unregister") } : new FinalizationRegistry((state) => {
    if (state.instance === __wbg_instance_id) {
      wasm.__wbindgen_destroy_closure(state.a, state.b);
    }
  });
  function getArrayJsValueFromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    const mem = getDataViewMemory0();
    const result = [];
    for (let i = ptr; i < ptr + 4 * len; i += 4) {
      result.push(wasm.__wbindgen_externrefs.get(mem.getUint32(i, true)));
    }
    wasm.__externref_drop_slice(ptr, len);
    return result;
  }
  __name(getArrayJsValueFromWasm0, "getArrayJsValueFromWasm0");
  var cachedDataViewMemory0 = null;
  function getDataViewMemory0() {
    if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || cachedDataViewMemory0.buffer.detached === void 0 && cachedDataViewMemory0.buffer !== wasm.memory.buffer) {
      cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
    }
    return cachedDataViewMemory0;
  }
  __name(getDataViewMemory0, "getDataViewMemory0");
  var cachedUint8ArrayMemory0 = null;
  function isLikeNone(x) {
    return x === void 0 || x === null;
  }
  __name(isLikeNone, "isLikeNone");
  function passArrayJsValueToWasm0(array, malloc) {
    const ptr = malloc(array.length * 4, 4) >>> 0;
    for (let i = 0; i < array.length; i++) {
      const add = addToExternrefTable0(array[i]);
      getDataViewMemory0().setUint32(ptr + 4 * i, add, true);
    }
    WASM_VECTOR_LEN = array.length;
    return ptr;
  }
  __name(passArrayJsValueToWasm0, "passArrayJsValueToWasm0");
  var __wbg_reinit_scheduled = false;
  var cachedTextDecoder = new TextDecoder("utf-8", { ignoreBOM: true, fatal: true });
  cachedTextDecoder.decode();
  var numBytesDecoded = 0;
  var cachedTextEncoder = new TextEncoder();
  if (!("encodeInto" in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function(arg, view) {
      const buf = cachedTextEncoder.encode(arg);
      view.set(buf);
      return {
        read: arg.length,
        written: buf.length
      };
    };
  }
  var WASM_VECTOR_LEN = 0;
  var wasm;
  function __wbg_set_wasm(val) {
    wasm = val;
  }
  __name(__wbg_set_wasm, "__wbg_set_wasm");

  // build/index.js
  __wbg_set_wasm(c4473d8da76029da8f1dbeb7a4c422565340_index_bg_exports);
  (void 0)();
})();
//# sourceMappingURL=index.js.map
