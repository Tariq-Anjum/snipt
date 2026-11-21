import { useId } from 'react';

interface MySmsGlassIconProps {
  size?: number;
  variant?: 'glass' | 'solid';
  className?: string;
}

/**
 * Bespoke Liquid Glass app icon for mySMS.
 * Uses multi-layer specular glass refraction, inner rim highlight,
 * frosted floating speech emblem, and subtle encrypted OTP shimmer.
 */
export function MySmsGlassIcon({
  size = 28,
  variant = 'glass',
  className = '',
}: MySmsGlassIconProps) {
  const uid = useId().replace(/:/g, '');
  const isGlass = variant === 'glass';

  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 64 64"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      aria-hidden="true"
      className={className}
    >
      <defs>
        {/* Outer liquid glass squircle gradient */}
        <linearGradient id={`${uid}-bg`} x1="6" y1="4" x2="58" y2="60" gradientUnits="userSpaceOnUse">
          {isGlass ? (
            <>
              <stop offset="0%" stopColor="#38BDF8" />
              <stop offset="48%" stopColor="#2563EB" />
              <stop offset="100%" stopColor="#1E3A8A" />
            </>
          ) : (
            <>
              <stop offset="0%" stopColor="#2563EB" />
              <stop offset="100%" stopColor="#1D4ED8" />
            </>
          )}
        </linearGradient>

        {/* Ambient refraction caustic glow inside glass */}
        <radialGradient id={`${uid}-caustic`} cx="0" cy="0" r="1" gradientUnits="userSpaceOnUse" gradientTransform="translate(18 14) rotate(42) scale(40 34)">
          <stop offset="0%" stopColor="#E0F2FE" stopOpacity={isGlass ? '0.68' : '0.24'} />
          <stop offset="55%" stopColor="#60A5FA" stopOpacity={isGlass ? '0.22' : '0.06'} />
          <stop offset="100%" stopColor="#1D4ED8" stopOpacity="0" />
        </radialGradient>

        {/* Bottom cyan-ice refraction bounce */}
        <radialGradient id={`${uid}-bounce`} cx="0" cy="0" r="1" gradientUnits="userSpaceOnUse" gradientTransform="translate(48 52) rotate(-135) scale(28)">
          <stop offset="0%" stopColor="#7DD3FC" stopOpacity={isGlass ? '0.52' : '0'} />
          <stop offset="100%" stopColor="#1E3A8A" stopOpacity="0" />
        </radialGradient>

        {/* Frosted translucent inner speech bubble */}
        <linearGradient id={`${uid}-bubble`} x1="16" y1="16" x2="48" y2="48" gradientUnits="userSpaceOnUse">
          <stop offset="0%" stopColor="#FFFFFF" stopOpacity={isGlass ? '0.94' : '1'} />
          <stop offset="100%" stopColor="#DBEAFE" stopOpacity={isGlass ? '0.68' : '0.94'} />
        </linearGradient>

        {/* Specular top glass sheen */}
        <linearGradient id={`${uid}-sheen`} x1="32" y1="4" x2="32" y2="30" gradientUnits="userSpaceOnUse">
          <stop offset="0%" stopColor="#FFFFFF" stopOpacity={isGlass ? '0.54' : '0.18'} />
          <stop offset="100%" stopColor="#FFFFFF" stopOpacity="0" />
        </linearGradient>

        {/* Specular rim stroke */}
        <linearGradient id={`${uid}-rim`} x1="8" y1="6" x2="56" y2="58" gradientUnits="userSpaceOnUse">
          <stop offset="0%" stopColor="#FFFFFF" stopOpacity={isGlass ? '0.85' : '0.35'} />
          <stop offset="45%" stopColor="#93C5FD" stopOpacity={isGlass ? '0.35' : '0.12'} />
          <stop offset="100%" stopColor="#FFFFFF" stopOpacity={isGlass ? '0.55' : '0.2'} />
        </linearGradient>
      </defs>

      {/* Squircle Base */}
      <rect x="3" y="3" width="58" height="58" rx="16" fill={`url(#${uid}-bg)`} />

      {/* Optical Caustic + Bounce */}
      <rect x="3" y="3" width="58" height="58" rx="16" fill={`url(#${uid}-caustic)`} />
      <rect x="3" y="3" width="58" height="58" rx="16" fill={`url(#${uid}-bounce)`} />

      {/* Upper curved specular lens reflection */}
      {isGlass && (
        <path
          d="M10 18C10 11.3726 15.3726 6 22 6H42C48.6274 6 54 11.3726 54 18V20C44 25 20 25 10 20V18Z"
          fill={`url(#${uid}-sheen)`}
        />
      )}

      {/* Floating Liquid-Glass Message Bubble */}
      <path
        d="M17 26C17 20.4772 21.4772 16 27 16H37C42.5228 16 47 20.4772 47 26V33C47 38.5228 42.5228 43 37 43H28.5L20.8 48.2C19.6 49 18 48.14 18 46.7V41.5C17.36 39.88 17 38.04 17 36V26Z"
        fill={`url(#${uid}-bubble)`}
        stroke="#FFFFFF"
        strokeOpacity={isGlass ? '0.9' : '0.4'}
        strokeWidth="1.2"
      />

      {/* Smart Organizer Signal Bars / Message Lines inside bubble */}
      <rect x="24" y="25.5" width="16" height="3.2" rx="1.6" fill="#1D4ED8" fillOpacity="0.88" />
      <rect x="24" y="31.8" width="10.5" height="3.2" rx="1.6" fill="#3B82F6" fillOpacity="0.82" />
      <circle cx="38.2" cy="33.4" r="2.1" fill="#0284C7" />

      {/* Outer Specular Glass Rim */}
      <rect
        x="3.75"
        y="3.75"
        width="56.5"
        height="56.5"
        rx="15.25"
        stroke={`url(#${uid}-rim)`}
        strokeWidth="1.5"
      />
    </svg>
  );
}
