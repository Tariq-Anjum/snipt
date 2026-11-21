import { Check, Copy, Download, FileCode2 } from 'lucide-react';
import { useState } from 'react';
import {
  buildBootstrapShellScript,
  RUST_WORKSPACE_FILES,
} from '../data/rustWorkspace';
import { MySmsGlassIcon } from './MySmsGlassIcon';
import type { DesignStyle } from './TopBar';

const ARCHITECTURE_LAYERS = [
  {
    name: 'Play Store Android Shell (Kotlin)',
    detail: 'Default SMS RoleManager, Multi-SIM SmsManager, Copy/Reply/Delete notifications, and FLAG_SECURE OTP isolation.',
  },
  {
    name: 'SAF Cloud Backup (Any Drive)',
    detail: 'Encrypts 10,000+ SMS locally with AES-256-GCM and saves via Android SAF to Google Drive, OneDrive, or Dropbox.',
  },
  {
    name: 'Offline Indian Languages (Free)',
    detail: 'Zero-server Hinglish rules + ML Kit On-Device Translation (~30 MB packs) for Hindi, Urdu, Marathi, Tamil, Telugu & Bengali.',
  },
  {
    name: 'Local Rust Store (v2: Wi-Fi Sync)',
    detail: 'Bundled SQLite FTS5 store for 10,000+ SMS at ₹0 server cost. Optional LAN laptop sync planned for v2.',
  },
];

interface RustCoreViewProps {
  designStyle: DesignStyle;
  onChangeDesignStyle: (style: DesignStyle) => void;
}

export function RustCoreView({
  designStyle,
  onChangeDesignStyle,
}: RustCoreViewProps) {
  const [selectedPath, setSelectedPath] = useState(RUST_WORKSPACE_FILES[2].path);
  const [copied, setCopied] = useState(false);

  const activeFile =
    RUST_WORKSPACE_FILES.find((f) => f.path === selectedPath) ??
    RUST_WORKSPACE_FILES[0];

  const handleCopyFile = () => {
    navigator.clipboard?.writeText(activeFile.content).catch(() => {});
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1800);
  };

  const handleDownloadScript = () => {
    const script = buildBootstrapShellScript();
    const blob = new Blob([script], { type: 'text/x-shellscript;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'mysms-rust-bootstrap.sh';
    document.body.appendChild(a);
    a.click();
    a.remove();
    URL.revokeObjectURL(url);
  };

  return (
    <main className="flex-1 overflow-y-auto smooth-scroll-layer bg-slate-50 dark:bg-slate-950 glass:bg-transparent px-4 md:px-8 py-6 pb-24 md:pb-10">
      <div className="max-w-7xl mx-auto space-y-6">
        <div className="flex flex-col sm:flex-row sm:items-end justify-between gap-4">
          <div>
            <h1 className="font-display text-2xl font-bold tracking-tight text-slate-900 dark:text-slate-100">
              Android &amp; Play Store Core Blueprint
            </h1>
            <p className="mt-1 text-sm text-slate-500 dark:text-slate-400">
              Zero-server architecture ready for personal use and free Play Store release: SQLite 10,000+ SMS store, SAF multi-cloud backup, Liquid Glass 120Hz Compose UI, and free offline Indian language translation.
            </p>
          </div>

          <button
            type="button"
            onClick={handleDownloadScript}
            className="inline-flex items-center gap-2 px-4 py-2 text-xs font-semibold text-white bg-blue-700 hover:bg-blue-800 dark:bg-blue-600 dark:hover:bg-blue-500 glass-cta rounded-lg transition-colors whitespace-nowrap self-start sm:self-auto"
          >
            <Download className="w-4 h-4" aria-hidden="true" />
            <span>Download Bootstrap Script (.sh)</span>
          </button>
        </div>

        {/* Liquid Glass Adaptive Icon & 120Hz Motion Spec Showcase */}
        <section
          aria-label="Liquid Glass app icon and adaptive refresh rate preview"
          className="bg-white dark:bg-slate-900 glass-panel border border-slate-200 dark:border-slate-800 rounded-2xl p-5 flex flex-col lg:flex-row lg:items-center justify-between gap-5"
        >
          <div className="flex items-center gap-4">
            <div className="p-2.5 rounded-2xl bg-slate-100/90 dark:bg-slate-950/80 border border-slate-200/80 dark:border-slate-800 shrink-0">
              <MySmsGlassIcon size={58} variant={designStyle} />
            </div>
            <div>
              <h2 className="font-display text-sm font-semibold text-slate-900 dark:text-slate-100">
                mySMS Adaptive App Icon &amp; 120Hz Liquid Glass Motion
              </h2>
              <p className="mt-0.5 text-xs text-slate-500 dark:text-slate-400 max-w-xl leading-relaxed">
                Multi-layer optical refraction squircle icon with specular rim highlights. Animations run exclusively on GPU transform/opacity layers synchronized to your screen&apos;s native 60Hz, 90Hz, or 120Hz+ LTPO adaptive refresh rate.
              </p>
            </div>
          </div>

          <div className="flex items-center gap-1 p-1 bg-slate-100 dark:bg-slate-800 rounded-lg self-start lg:self-center shrink-0">
            <button
              type="button"
              onClick={() => onChangeDesignStyle('glass')}
              aria-pressed={designStyle === 'glass'}
              className={`px-3 py-1.5 text-xs font-medium rounded-md transition-colors whitespace-nowrap ${
                designStyle === 'glass'
                  ? 'bg-white dark:bg-slate-900 text-blue-700 dark:text-blue-300 shadow-xs'
                  : 'text-slate-600 dark:text-slate-400'
              }`}
            >
              Liquid Glass
            </button>
            <button
              type="button"
              onClick={() => onChangeDesignStyle('solid')}
              aria-pressed={designStyle === 'solid'}
              className={`px-3 py-1.5 text-xs font-medium rounded-md transition-colors whitespace-nowrap ${
                designStyle === 'solid'
                  ? 'bg-white dark:bg-slate-900 text-blue-700 dark:text-blue-300 shadow-xs'
                  : 'text-slate-600 dark:text-slate-400'
              }`}
            >
              Crisp Solid
            </button>
          </div>
        </section>

        {/* Architecture Layers Strip */}
        <section
          aria-label="Rust-first architecture layers"
          className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4"
        >
          {ARCHITECTURE_LAYERS.map((layer, index) => (
            <div
              key={layer.name}
              className="bg-white dark:bg-slate-900 glass-panel border border-slate-200 dark:border-slate-800 rounded-2xl p-4"
            >
              <p className="font-display text-xs font-semibold text-blue-700 dark:text-blue-400">
                0{index + 1}. Layer
              </p>
              <h2 className="mt-1 font-display text-sm font-semibold text-slate-900 dark:text-slate-100">
                {layer.name}
              </h2>
              <p className="mt-1.5 text-xs leading-relaxed text-slate-600 dark:text-slate-400">
                {layer.detail}
              </p>
            </div>
          ))}
        </section>

        {/* Interactive Workspace File Explorer & Source Viewer */}
        <section
          aria-label="Rust workspace files"
          className="bg-white dark:bg-slate-900 glass-panel border border-slate-200 dark:border-slate-800 rounded-2xl overflow-hidden grid grid-cols-1 lg:grid-cols-[310px_1fr]"
        >
          <div className="border-b lg:border-b-0 lg:border-r border-slate-200 dark:border-slate-800 p-3 space-y-1">
            <h2 className="px-2.5 py-1.5 text-xs font-semibold text-slate-500 dark:text-slate-400">
              Workspace Files ({RUST_WORKSPACE_FILES.length})
            </h2>
            {RUST_WORKSPACE_FILES.map((file) => {
              const isSelected = file.path === activeFile.path;
              return (
                <button
                  key={file.path}
                  type="button"
                  onClick={() => setSelectedPath(file.path)}
                  aria-current={isSelected ? 'true' : undefined}
                  className={`w-full text-left px-2.5 py-2 rounded-lg transition-colors flex items-start gap-2.5 ${
                    isSelected
                      ? 'bg-blue-50 text-blue-800 dark:bg-blue-950/60 dark:text-blue-300'
                      : 'text-slate-700 hover:bg-slate-100 dark:text-slate-300 dark:hover:bg-slate-800/70'
                  }`}
                >
                  <FileCode2
                    className="w-4 h-4 mt-0.5 shrink-0 opacity-75"
                    aria-hidden="true"
                  />
                  <div className="min-w-0 flex-1">
                    <div className="font-mono text-xs font-medium truncate">
                      {file.path}
                    </div>
                    <div className="text-[11px] text-slate-500 dark:text-slate-400">
                      {file.crate} · {file.language}
                    </div>
                  </div>
                </button>
              );
            })}
          </div>

          <div className="min-w-0 flex flex-col">
            <div className="px-5 py-3.5 border-b border-slate-200 dark:border-slate-800 flex flex-wrap items-center justify-between gap-3">
              <div className="min-w-0">
                <h3 className="font-mono text-sm font-semibold text-slate-900 dark:text-slate-100 truncate">
                  {activeFile.path}
                </h3>
                <p className="text-xs text-slate-500 dark:text-slate-400 mt-0.5">
                  {activeFile.summary}
                </p>
              </div>

              <button
                type="button"
                onClick={handleCopyFile}
                className="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium text-slate-700 dark:text-slate-200 bg-slate-100 hover:bg-slate-200 dark:bg-slate-800 dark:hover:bg-slate-700 rounded-lg transition-colors whitespace-nowrap shrink-0"
              >
                {copied ? (
                  <>
                    <Check className="w-3.5 h-3.5 text-emerald-600 dark:text-emerald-400" aria-hidden="true" />
                    <span>Copied</span>
                  </>
                ) : (
                  <>
                    <Copy className="w-3.5 h-3.5" aria-hidden="true" />
                    <span>Copy file</span>
                  </>
                )}
              </button>
            </div>

            <div className="bg-slate-950 text-slate-100 p-5 overflow-x-auto max-h-[560px] overflow-y-auto">
              <pre className="font-mono text-xs leading-6">
                <code>{activeFile.content}</code>
              </pre>
            </div>
          </div>
        </section>
      </div>
    </main>
  );
}
