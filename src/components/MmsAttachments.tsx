import { ImageOff, Paperclip, X } from 'lucide-react';
import { memo, useEffect, useState } from 'react';
import { createPortal } from 'react-dom';
import { mmsImage, type AttachmentMeta } from '../api';

/** Types the core may return; anything else is refused before a blob is made. */
const IMAGE_TYPES = new Set(['image/jpeg', 'image/png', 'image/gif', 'image/webp', 'image/bmp']);

/**
 * Loads an attachment through the `mms_image` command into a `blob:` URL
 * (CSP img-src allows blob:). The webview never sees a file path or content
 * URI; the URL is revoked when the component unmounts.
 */
function useImageUrl(partId: number, thumb: boolean, enabled = true) {
  const [url, setUrl] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    if (!enabled) return;
    let alive = true;
    let made: string | null = null;
    setFailed(false);
    mmsImage(partId, thumb)
      .then((img) => {
        if (!alive) return;
        if (!IMAGE_TYPES.has(img.contentType)) throw new Error('type');
        const bin = atob(img.dataB64);
        const bytes = new Uint8Array(bin.length);
        for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
        made = URL.createObjectURL(new Blob([bytes], { type: img.contentType }));
        setUrl(made);
      })
      .catch(() => alive && setFailed(true));
    return () => {
      alive = false;
      if (made) URL.revokeObjectURL(made);
      setUrl(null);
    };
  }, [partId, thumb, enabled]);
  return { url, failed };
}

function formatSize(n: number): string {
  if (n >= 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
  if (n >= 1024) return `${Math.round(n / 1024)} KB`;
  return `${n} B`;
}

function Viewer({ att, onClose }: { att: AttachmentMeta; onClose: () => void }) {
  const { url, failed } = useImageUrl(att.id, false);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && onClose();
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);
  // Portal to <body>: thread rows are transformed (virtualizer), which would
  // make `position: fixed` relative to the row instead of the viewport.
  return createPortal(
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Image attachment"
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/85 p-4"
      onClick={onClose}
    >
      <button
        type="button"
        onClick={onClose}
        aria-label="Close image"
        className="absolute top-4 right-4 rounded-full bg-white/10 p-2 text-white hover:bg-white/20"
      >
        <X className="w-5 h-5" aria-hidden="true" />
      </button>
      {url ? (
        <img
          src={url}
          alt={att.name ?? 'Image attachment'}
          className="max-h-full max-w-full object-contain"
          onClick={(e) => e.stopPropagation()}
        />
      ) : (
        <span className="text-sm text-white/80">{failed ? 'Image could not be shown.' : 'Loading...'}</span>
      )}
    </div>,
    document.body
  );
}

function ImageThumb({ att }: { att: AttachmentMeta }) {
  const [open, setOpen] = useState(false);
  // Only images the platform thumbnailer validated are served (R5 #7); the
  // thread refreshes (sms-changed) when a thumbnail arrives.
  const { url, failed } = useImageUrl(att.id, true, att.hasThumb);
  return (
    <>
      <button
        type="button"
        onClick={() => att.hasThumb && setOpen(true)}
        aria-label={`View image ${att.name ?? ''}`.trim()}
        className="block overflow-hidden rounded-xl border border-slate-200 dark:border-slate-700 bg-slate-100 dark:bg-slate-800"
      >
        {url ? (
          <img src={url} alt={att.name ?? 'Image attachment'} className="block max-h-64 max-w-full object-cover" />
        ) : (
          <span className="flex h-32 w-48 items-center justify-center gap-1.5 text-xs text-slate-500 dark:text-slate-400">
            {failed || !att.hasThumb ? (
              <>
                <ImageOff className="w-4 h-4" aria-hidden="true" />{' '}
                {att.hasThumb ? 'Image unavailable' : 'Image not shown (too large or still processing)'}
              </>
            ) : (
              'Loading image...'
            )}
          </span>
        )}
      </button>
      {open && <Viewer att={att} onClose={() => setOpen(false)} />}
    </>
  );
}

/** MMS attachments above the message text: images (tap to view), others as a chip. */
export const MmsAttachments = memo(function MmsAttachments({
  attachments,
}: {
  attachments: AttachmentMeta[];
}) {
  return (
    <div className="mb-2 flex flex-col gap-2">
      {attachments.map((a) =>
        a.kind === 'image' ? (
          <ImageThumb key={a.id} att={a} />
        ) : (
          <div
            key={a.id}
            className="inline-flex items-center gap-2 rounded-lg border border-dashed border-slate-300 dark:border-slate-700 px-3 py-2 text-xs text-slate-600 dark:text-slate-300"
          >
            <Paperclip className="w-3.5 h-3.5 shrink-0" aria-hidden="true" />
            <span>
              Attachment not supported
              {a.name ? ` · ${a.name}` : ''}
              {a.size > 0 ? ` · ${formatSize(a.size)}` : ''}
            </span>
          </div>
        )
      )}
    </div>
  );
});
