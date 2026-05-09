interface Props {
  toml: string;
  onClose: () => void;
}

export function SaveModal({ toml, onClose }: Props) {
  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/50 p-4"
      onClick={onClose}
    >
      <div
        className="flex max-h-[80vh] w-full max-w-3xl flex-col gap-3 rounded-lg bg-white p-4 shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between">
          <h2 className="text-lg font-semibold">Template TOML</h2>
          <button
            onClick={onClose}
            className="rounded px-3 py-1 text-sm text-slate-600 hover:bg-slate-100"
          >
            close
          </button>
        </div>
        <p className="text-xs text-slate-500">
          This is what `Shim.serializeTemplateToml` would write. The mock
          impl prints a JSON preview; the real WASM shim emits the same
          TOML the desktop loads.
        </p>
        <pre className="flex-1 overflow-auto rounded bg-slate-900 p-3 text-xs text-slate-100">
          {toml}
        </pre>
        <div className="flex justify-end gap-2">
          <button
            onClick={() => navigator.clipboard.writeText(toml)}
            className="rounded border border-slate-300 px-3 py-1.5 text-sm hover:bg-slate-100"
          >
            copy
          </button>
          <button
            onClick={onClose}
            className="rounded bg-slate-900 px-3 py-1.5 text-sm text-white hover:bg-slate-800"
          >
            done
          </button>
        </div>
      </div>
    </div>
  );
}
