export type DetectedLanguageCode =
  | 'en'
  | 'hi'
  | 'ur'
  | 'hinglish'
  | 'mr'
  | 'bn'
  | 'ta'
  | 'te';

export interface LanguageInfo {
  code: DetectedLanguageCode;
  label: string;
  nativeLabel: string;
  isRtl?: boolean;
}

export const SUPPORTED_LANGUAGES: Record<DetectedLanguageCode, LanguageInfo> = {
  en: { code: 'en', label: 'English', nativeLabel: 'English' },
  hi: { code: 'hi', label: 'Hindi', nativeLabel: 'हिन्दी' },
  ur: { code: 'ur', label: 'Urdu', nativeLabel: 'اردو', isRtl: true },
  hinglish: { code: 'hinglish', label: 'Hinglish', nativeLabel: 'Hinglish' },
  mr: { code: 'mr', label: 'Marathi', nativeLabel: 'मराठी' },
  bn: { code: 'bn', label: 'Bengali', nativeLabel: 'বাংলা' },
  ta: { code: 'ta', label: 'Tamil', nativeLabel: 'தமிழ்' },
  te: { code: 'te', label: 'Telugu', nativeLabel: 'తెలుగు' },
};

export interface TranslationResult {
  language: LanguageInfo;
  needsTranslation: boolean;
  translatedText: string;
  originalText: string;
  engineLabel: string;
}

const DEVANAGARI_RE = /[\u0900-\u097F]/g;
const URDU_ARABIC_RE = /[\u0600-\u06FF]/g;
const BENGALI_RE = /[\u0980-\u09FF]/g;
const TAMIL_RE = /[\u0B80-\u0BFF]/g;
const TELUGU_RE = /[\u0C00-\u0C7F]/g;

const MARATHI_MARKERS = [
  'ळ',
  'तुमचे',
  'तुमच्या',
  'खात्यातून',
  'रुपये',
  'झाले',
  'आहे',
  'नाही',
  'कृपया',
  'पोहोचेल',
  'आज',
];

const HINGLISH_MARKERS = new Set([
  'aapka',
  'aapke',
  'aapki',
  'khata',
  'khaate',
  'paise',
  'paisa',
  'bhai',
  'yaar',
  'kal',
  'aaj',
  'abhi',
  'milte',
  'bhej',
  'bheja',
  'bhejna',
  'kya',
  'hai',
  'hain',
  'nahi',
  'nhi',
  'karo',
  'karein',
  'karna',
  'shukriya',
  'dhanyawad',
  'kat',
  'gaye',
  'gaya',
  'subah',
  'shaam',
  'raat',
  'kitne',
  'baje',
  'ghar',
  'pahunch',
  'pohonch',
  'raha',
  'rahi',
  'hoon',
  'hun',
  'kisi',
  'mat',
  'batayein',
  'batana',
  'hoga',
  'hogi',
  'mein',
  'se',
  'ko',
  'ka',
  'ki',
  'ke',
  'par',
  'liye',
  'kripya',
  'jaldi',
  'theek',
  'thik',
  'chalo',
  'aana',
  'milenge',
  'khana',
  'paani',
  'rupaye',
  'jama',
  'nikale',
]);

/**
 * Lightweight zero-dependency script & language detector (<0.1ms per SMS).
 * Recognizes English, Hindi, Urdu, Hinglish, Marathi, Bengali, Tamil, and Telugu.
 */
export function detectMessageLanguage(text: string): LanguageInfo {
  const trimmed = text.trim();
  if (!trimmed) return SUPPORTED_LANGUAGES.en;

  const urMatches = trimmed.match(URDU_ARABIC_RE);
  if (urMatches && urMatches.length >= 2) {
    return SUPPORTED_LANGUAGES.ur;
  }

  const taMatches = trimmed.match(TAMIL_RE);
  if (taMatches && taMatches.length >= 2) {
    return SUPPORTED_LANGUAGES.ta;
  }

  const teMatches = trimmed.match(TELUGU_RE);
  if (teMatches && teMatches.length >= 2) {
    return SUPPORTED_LANGUAGES.te;
  }

  const bnMatches = trimmed.match(BENGALI_RE);
  if (bnMatches && bnMatches.length >= 2) {
    return SUPPORTED_LANGUAGES.bn;
  }

  const devMatches = trimmed.match(DEVANAGARI_RE);
  if (devMatches && devMatches.length >= 2) {
    // Distinguish Marathi from Hindi via characteristic Marathi tokens / ळ
    let marathiHits = 0;
    if (trimmed.includes('ळ')) marathiHits += 2;
    for (const marker of MARATHI_MARKERS) {
      if (marker !== 'ळ' && trimmed.includes(marker)) {
        marathiHits += 1;
      }
    }
    if (marathiHits >= 2 && !trimmed.includes('है') && !trimmed.includes('आपका')) {
      return SUPPORTED_LANGUAGES.mr;
    }
    return SUPPORTED_LANGUAGES.hi;
  }

  // Latin script: check for Romanized Hindi/Urdu (Hinglish)
  const words = trimmed
    .toLowerCase()
    .replace(/[^a-z\s]/g, ' ')
    .split(/\s+/)
    .filter(Boolean);

  let hinglishHits = 0;
  let strongHinglishHits = 0;
  const strongSet = new Set([
    'aapka',
    'aapke',
    'khata',
    'paise',
    'bhai',
    'yaar',
    'milte',
    'bhej',
    'bheja',
    'kya',
    'nahi',
    'nhi',
    'karo',
    'karein',
    'shukriya',
    'dhanyawad',
    'subah',
    'shaam',
    'kitne',
    'baje',
    'ghar',
    'pahunch',
    'raha',
    'hoon',
    'batayein',
    'kripya',
    'jaldi',
    'theek',
    'chalo',
    'milenge',
    'rupaye',
    'khana',
    'aaj',
    'kal',
    'hai',
    'hain',
  ]);

  for (const w of words) {
    if (HINGLISH_MARKERS.has(w)) hinglishHits += 1;
    if (strongSet.has(w)) strongHinglishHits += 1;
  }

  if (strongHinglishHits >= 2 || (strongHinglishHits >= 1 && hinglishHits >= 3)) {
    return SUPPORTED_LANGUAGES.hinglish;
  }

  return SUPPORTED_LANGUAGES.en;
}

type PhraseRule = [RegExp, string];

const HINDI_RULES: PhraseRule[] = [
  [/आपका\s+ओटीपी/g, 'Your OTP'],
  [/आपका\s+OTP/gi, 'Your OTP'],
  [/किसी\s+के\s+साथ\s+साझा\s+न\s+करें/g, 'Do not share with anyone'],
  [/किसी\s+को\s+न\s+बताएं/g, 'Do not tell anyone'],
  [/शेयर\s+न\s+करें/g, 'Do not share'],
  [/साझा\s+न\s+करें/g, 'Do not share'],
  [/के\s+लिए\s+मान्य\s+है/g, 'is valid for'],
  [/मिनट\s+के\s+लिए\s+मान्य/g, 'valid for minutes'],
  [/मिनट/g, 'mins'],
  [/आपके\s+खाते\s+से/g, 'From your account'],
  [/आपके\s+बैंक\s+खाते\s+से/g, 'From your bank account'],
  [/आपके\s+खाते\s+में/g, 'Into your account'],
  [/यूपीआई\s+के\s+माध्यम\s+से/g, 'via UPI'],
  [/यूपीआई/g, 'UPI'],
  [/क्रेडिट\s+कार्ड/g, 'Credit Card'],
  [/डेबिट\s+हुए\s+हैं/g, 'has been debited'],
  [/डेबिट\s+हुए/g, 'debited'],
  [/क्रेडिट\s+हुए\s+हैं/g, 'has been credited'],
  [/क्रेडिट\s+हुए/g, 'credited'],
  [/कट\s+गए\s+हैं/g, 'has been debited'],
  [/जमा\s+हुए\s+हैं/g, 'has been credited'],
  [/उपलब्ध\s+बैलेंस/g, 'Available Balance'],
  [/बकाया\s+राशि/g, 'Balance Amount'],
  [/रुपये|रुपए|रु\./g, 'Rs.'],
  [/आपका\s+ऑर्डर/g, 'Your order'],
  [/आज\s+शाम\s+(\d+)\s+बजे\s+तक\s+डिलीवर\s+हो\s+जाएगा/g, 'will be delivered by $1 PM today evening'],
  [/आज\s+शाम/g, 'today evening'],
  [/आज\s+सुबह/g, 'today morning'],
  [/आज\s+तक/g, 'by today'],
  [/आज/g, 'today'],
  [/कल/g, 'tomorrow'],
  [/डिलीवर\s+हो\s+जाएगा/g, 'will be delivered'],
  [/डिलीवरी\s+के\s+लिए\s+निकल\s+चुका\s+है/g, 'is out for delivery'],
  [/कृपया\s+उपलब्ध\s+रहें/g, 'Please stay available'],
  [/कृपया/g, 'Please'],
  [/धन्यवाद/g, 'Thank you'],
  [/लेन-देन/g, 'transaction'],
  [/भुगतान/g, 'payment'],
  [/बिल/g, 'bill'],
  [/अंतिम\s+तिथि/g, 'due date'],
  [/ट्रेन/g, 'Train'],
  [/टिकट/g, 'ticket'],
  [/कन्फर्म\s+हो\s+गया\s+है/g, 'is confirmed'],
  [/नमस्ते/g, 'Hello'],
  [/घर\s+पहुँच\s+गया\s+हूँ/g, 'I have reached home'],
  [/पहुँच\s+रहा\s+हूँ/g, 'reaching soon'],
  [/मिलते\s+हैं/g, 'let us meet'],
  [/खाना\s+खा\s+लिया/g, 'had dinner'],
  [/है/g, 'is'],
  [/हैं/g, 'are'],
  [/को/g, 'to'],
  [/के\s+लिए/g, 'for'],
  [/पर/g, 'on'],
  [/से/g, 'from'],
  [/में/g, 'in'],
  [/।/g, '.'],
];

const URDU_RULES: PhraseRule[] = [
  [/السلام\s+علیکم/g, 'Assalamu Alaikum (Hello)'],
  [/وعلیکم\s+السلام/g, 'Walaikum Assalam (Hello)'],
  [/آپ\s+کا\s+او\s*ٹی\s*پی/g, 'Your OTP'],
  [/آپ\s+کا\s+OTP/gi, 'Your OTP'],
  [/کسی\s+کو\s+نہ\s+بتائیں/g, 'Do not share with anyone'],
  [/شیئر\s+نہ\s+کریں/g, 'Do not share'],
  [/آپ\s+کے\s+اکاؤنٹ\s+سے/g, 'From your account'],
  [/آپ\s+کے\s+اکاؤنٹ\s+میں/g, 'Into your account'],
  [/روپے/g, 'Rs.'],
  [/کٹ\s+گئے\s+ہیں/g, 'have been debited'],
  [/جمع\s+ہو\s+گئے\s+ہیں/g, 'have been credited'],
  [/بقیہ\s+بیلنس/g, 'Available Balance'],
  [/میں\s+گھر\s+پہنچ\s+گیا\s+ہوں/g, 'I have reached home'],
  [/گھر\s+پہنچ\s+کر\s+فون\s+کرنا/g, 'call after reaching home'],
  [/کیا\s+حال\s+ہے/g, 'How are you'],
  [/کل\s+شام\s+(\d+)\s+بجے\s+رات\s+کا\s+کھانا\s+ملتے\s+ہیں/g, 'let us meet for dinner tomorrow evening at $1 PM'],
  [/کل\s+شام/g, 'tomorrow evening'],
  [/آج\s+شام/g, 'today evening'],
  [/آج\s+رات/g, 'tonight'],
  [/شام\s+کو/g, 'in the evening'],
  [/صبح/g, 'morning'],
  [/رات\s+کا\s+کھانا/g, 'dinner'],
  [/کھانا/g, 'food'],
  [/ملتے\s+ہیں/g, 'let us meet'],
  [/بجے/g, "o'clock"],
  [/ان\s+شاء\s+اللہ/g, 'God willing'],
  [/شکریہ/g, 'Thank you'],
  [/براہ\s+کرم/g, 'Please'],
  [/بھیج\s+دیے\s+ہیں/g, 'have been sent'],
  [/بھیج\s+دیا\s+ہے/g, 'has been sent'],
  [/بھیج\s+دو/g, 'please send'],
  [/فون\s+کرنا/g, 'please call'],
  [/پیسے/g, 'money'],
  [/آرڈر/g, 'order'],
  [/آج/g, 'today'],
  [/کل/g, 'tomorrow'],
  [/اور/g, 'and'],
  [/کی\s+طرف\s+سے/g, 'from'],
  [/کے\s+لیے/g, 'for'],
  [/تک/g, 'by'],
  [/ہے/g, 'is'],
  [/ہیں/g, 'are'],
  [/،/g, ','],
  [/؟/g, '?'],
  [/۔/g, '.'],
];

const HINGLISH_RULES: PhraseRule[] = [
  [/\baapka\s+otp\b/gi, 'Your OTP'],
  [/\bkisi\s+ko\s+mat\s+batayein\b/gi, 'do not share with anyone'],
  [/\bkisi\s+se\s+share\s+mat\s+karein\b/gi, 'do not share with anyone'],
  [/\bshare\s+mat\s+karo\b/gi, 'do not share'],
  [/\bmat\s+batana\b/gi, 'do not tell'],
  [/\baapke\s+khate?\s+se\b/gi, 'From your account'],
  [/\baapke\s+account\s+se\b/gi, 'From your account'],
  [/\bkat\s+gaye\s+hain\b/gi, 'have been debited'],
  [/\bjama\s+ho\s+gaye\s+hain\b/gi, 'have been credited'],
  [/\bbhai\b/gi, 'brother'],
  [/\byaar\b/gi, 'friend'],
  [/\bkya\s+haal\s+hai\b/gi, 'how are you'],
  [/\bghar\s+pahunch\s+kar\s+call\s+karna\b/gi, 'Call me once you reach home'],
  [/\bghar\s+pahunch\s+gaya\s+hoon\b/gi, 'I reached home'],
  [/\bghar\s+pahunch\s+raha\s+hoon\b/gi, 'I am reaching home'],
  [/\bpahunch\s+gaya\b/gi, 'reached'],
  [/\bpaise\s+bhej\s+diye\s+hain\b/gi, 'I have sent the money'],
  [/\bpaise\s+bhej\s+diya\b/gi, 'sent the money'],
  [/\bpaise\s+bhej\s+do\b/gi, 'please send the money'],
  [/\bbhej\s+diye\s+hain\b/gi, 'have been sent'],
  [/\bbhej\s+do\b/gi, 'please send'],
  [/\bkal\s+shaam\b/gi, 'tomorrow evening'],
  [/\baaj\s+shaam\b/gi, 'today evening'],
  [/\baaj\s+raat\b/gi, 'tonight'],
  [/\bkal\s+subah\b/gi, 'tomorrow morning'],
  [/\baaj\s+subah\b/gi, 'today morning'],
  [/\bkitne\s+baje\b/gi, 'what time'],
  [/\bmilte\s+hain\b/gi, 'let us meet'],
  [/\bmilenge\b/gi, 'we will meet'],
  [/\bbaje\b/gi, "o'clock"],
  [/\bkhana\s+khane\b/gi, 'for dinner'],
  [/\bkhana\b/gi, 'food'],
  [/\bshukriya\b/gi, 'thank you'],
  [/\bdhanyawad\b/gi, 'thank you'],
  [/\bkripya\b/gi, 'please'],
  [/\btheek\s+hai\b/gi, 'okay'],
  [/\bthik\s+hai\b/gi, 'okay'],
  [/\bjaldi\s+aana\b/gi, 'come soon'],
  [/\bjaldi\b/gi, 'quickly'],
  [/\bdeliver\s+ho\s+jayega\b/gi, 'will be delivered'],
  [/\bho\s+gaya\s+hai\b/gi, 'is done'],
  [/\bho\s+jayega\b/gi, 'will be done'],
  [/\bkar\s+diya\s+hai\b/gi, 'have done'],
  [/\bkar\s+do\b/gi, 'please do'],
  [/\bkaro\b/gi, 'do'],
  [/\bpaise\b/gi, 'money'],
  [/\brupaye\b/gi, 'Rs.'],
  [/\baaj\b/gi, 'today'],
  [/\bkal\b/gi, 'tomorrow'],
  [/\babhi\b/gi, 'right now'],
  [/\bghar\b/gi, 'home'],
  [/\bmein\b/gi, 'in'],
  [/\bse\b/gi, 'via'],
  [/\bko\b/gi, 'at'],
  [/\bke\s+liye\b/gi, 'for'],
  [/\bnhi\b|\bnahi\b/gi, 'not'],
  [/\bhain\b|\bhai\b/gi, 'is'],
  [/\bhoon\b|\bhun\b/gi, 'am'],
];

const MARATHI_RULES: PhraseRule[] = [
  [/तुमच्या\s+खात्यातून/g, 'From your account'],
  [/तुमचे\s+पार्सल/g, 'Your parcel'],
  [/तुमची\s+ऑर्डर/g, 'Your order'],
  [/रुपये/g, 'Rs.'],
  [/डेबिट\s+झाले\s+आहेत/g, 'have been debited'],
  [/क्रेडिट\s+झाले\s+आहेत/g, 'have been credited'],
  [/आज\s+संध्याकाळी\s+(\d+)\s+पर्यंत\s+पोहोचेल/g, 'will arrive by $1 PM today evening'],
  [/आज\s+संध्याकाळी/g, 'today evening'],
  [/आज/g, 'today'],
  [/पर्यंत\s+पोहोचेल/g, 'will arrive by'],
  [/पोहोचेल/g, 'will be delivered'],
  [/उपलब्ध\s+शिल्लक/g, 'Available Balance'],
  [/कृपया\s+फोन\s+जवळ\s+ठेवा/g, 'Please keep your phone nearby'],
  [/कृपया/g, 'Please'],
  [/कोणालाही\s+सांगू\s+नका/g, 'do not tell anyone'],
  [/धन्यवाद/g, 'Thank you'],
  [/आहे/g, 'is'],
];

const TAMIL_RULES: PhraseRule[] = [
  [/உங்கள்\s+கணக்கிலிருந்து/g, 'From your account'],
  [/உங்கள்\s+ஆர்டர்/g, 'Your order'],
  [/உங்கள்\s+OTP/gi, 'Your OTP'],
  [/இன்று\s+மாலை/g, 'today evening'],
  [/இன்று/g, 'today'],
  [/ரூபாய்|ரூ\./g, 'Rs.'],
  [/வழங்கப்படும்/g, 'will be delivered'],
  [/டெலிவரி\s+செய்யப்படும்/g, 'will be delivered'],
  [/யாருடனும்\s+பகிர\s+வேண்டாம்/g, 'Do not share with anyone'],
  [/நன்றி/g, 'Thank you'],
  [/தயவுசெய்து/g, 'Please'],
];

const TELUGU_RULES: PhraseRule[] = [
  [/మీ\s+ఖాతా\s+నుండి/g, 'From your account'],
  [/మీ\s+ఆర్డర్/g, 'Your order'],
  [/మీ\s+OTP/gi, 'Your OTP'],
  [/ఈరోజు\s+సాయంత్రం/g, 'today evening'],
  [/ఈరోజు/g, 'today'],
  [/రూపాయలు|రూ\./g, 'Rs.'],
  [/డెలివరీ\s+అవుతుంది/g, 'will be delivered'],
  [/ఎవరితోనూ\s+పంచుకోవద్దు/g, 'Do not share with anyone'],
  [/ధన్యవాదాలు/g, 'Thank you'],
];

const BENGALI_RULES: PhraseRule[] = [
  [/আপনার\s+অ্যাকাউন্ট\s+থেকে/g, 'From your account'],
  [/আপনার\s+অর্ডার/g, 'Your order'],
  [/আপনার\s+OTP/gi, 'Your OTP'],
  [/আজ\s+সন্ধ্যায়/g, 'today evening'],
  [/আজ/g, 'today'],
  [/টাকা/g, 'Rs.'],
  [/ডেলিভারি\s+করা\s+হবে/g, 'will be delivered'],
  [/কারো\s+সাথে\s+শেয়ার\s+করবেন\s+না/g, 'Do not share with anyone'],
  [/ধন্যবাদ/g, 'Thank you'],
];

const RULE_TABLE: Record<Exclude<DetectedLanguageCode, 'en'>, PhraseRule[]> = {
  hi: HINDI_RULES,
  ur: URDU_RULES,
  hinglish: HINGLISH_RULES,
  mr: MARATHI_RULES,
  ta: TAMIL_RULES,
  te: TELUGU_RULES,
  bn: BENGALI_RULES,
};

function normalizeTranslatedSentence(text: string): string {
  const cleaned = text
    .replace(/\s+/g, ' ')
    .replace(/\s+([.,!?])/g, '$1')
    .replace(/Rs\.\s+/g, 'Rs.')
    .trim();
  if (!cleaned) return cleaned;
  return cleaned.charAt(0).toUpperCase() + cleaned.slice(1);
}

/**
 * Translates Hindi, Urdu, Hinglish, Marathi, Tamil, Telugu, and Bengali SMS
 * to English completely offline (0 network requests, 0 API fees, <1ms CPU).
 * Exact amounts, OTPs, account digits, and PNRs are preserved verbatim.
 */
export function translateSmsToEnglish(text: string): TranslationResult {
  const language = detectMessageLanguage(text);

  if (language.code === 'en') {
    return {
      language,
      needsTranslation: false,
      translatedText: text,
      originalText: text,
      engineLabel: 'Native English',
    };
  }

  let output = text;
  const rules = RULE_TABLE[language.code];
  for (const [pattern, replacement] of rules) {
    output = output.replace(pattern, replacement);
  }

  const normalized = normalizeTranslatedSentence(output);

  return {
    language,
    needsTranslation: true,
    translatedText: normalized,
    originalText: text,
    engineLabel: 'Offline Lightweight Pack (0 KB/req)',
  };
}
