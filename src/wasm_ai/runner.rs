use wasmtime::*;
use crate::wasm_ai::interface::{WasmCommand, parse_wasm_commands};

pub struct WasmAiRunner {
    store: Store<()>,
    instance: Instance,
    memory: Memory,
    alloc_fn: TypedFunc<u32, u32>,
    on_tick_fn: TypedFunc<(u32, u32), u64>,
}

impl WasmAiRunner {
    pub fn new(wasm_bytes: &[u8]) -> Result<Self, anyhow::Error> {
        let engine = Engine::new(
            Config::new()
                .consume_fuel(true)
                .wasm_bulk_memory(true)
        )?;

        let module = Module::new(&engine, wasm_bytes)?;
        let mut store = Store::new(&engine, ());

        // Give the module a fuel budget per instantiation
        store.set_fuel(1_000_000)?;

        let instance = Instance::new(&mut store, &module, &[])?;

        let memory = instance.get_memory(&mut store, "memory")
            .ok_or_else(|| anyhow::anyhow!("WASM module must export 'memory'"))?;

        let alloc_fn = instance.get_typed_func::<u32, u32>(&mut store, "alloc")?;
        let on_tick_fn = instance.get_typed_func::<(u32, u32), u64>(&mut store, "on_tick")?;

        Ok(Self { store, instance, memory, alloc_fn, on_tick_fn })
    }

    /// Call the WASM module's on_tick with serialized game state, returns parsed commands
    pub fn tick(&mut self, game_state: &[u8]) -> Result<Vec<WasmCommand>, anyhow::Error> {
        // Reset fuel each tick
        self.store.set_fuel(1_000_000)?;

        let state_len = game_state.len() as u32;

        // Allocate memory in WASM for the game state
        let state_ptr = self.alloc_fn.call(&mut self.store, state_len)?;

        // Write game state into WASM memory
        self.memory.data_mut(&mut self.store)[state_ptr as usize..(state_ptr + state_len) as usize]
            .copy_from_slice(game_state);

        // Call on_tick
        let result = self.on_tick_fn.call(&mut self.store, (state_ptr, state_len))?;

        // Result encodes ptr (high 32 bits) and len (low 32 bits)
        let cmd_ptr = (result >> 32) as u32;
        let cmd_len = (result & 0xFFFFFFFF) as u32;

        if cmd_len == 0 {
            return Ok(Vec::new());
        }

        // Read commands from WASM memory
        let cmd_data = self.memory.data(&self.store)[cmd_ptr as usize..(cmd_ptr + cmd_len) as usize].to_vec();

        Ok(parse_wasm_commands(&cmd_data))
    }
}
