import {
  CATEGORY_COUNT,
  CATEGORY_LIST,
  type Classification,
  type ParsedSms,
} from './types';

/**
 * Aggregates per-message probability distributions across a conversation thread
 * with exponential recency weighting (Organiso's 93% -> 99% consensus trick).
 */
export function classifyThread(messages: ParsedSms[]): Classification {
  if (messages.length === 0) {
    return {
      category: 'Personal',
      confidence: 1.0,
      scores: [1, 0, 0, 0, 0],
      reason: 'empty_thread',
    };
  }

  const slice = messages.length > 12 ? messages.slice(messages.length - 12) : messages;
  const acc: [number, number, number, number, number] = [0, 0, 0, 0, 0];
  let weightSum = 0;

  for (let i = 0; i < slice.length; i += 1) {
    const msg = slice[i];
    const recency = Math.pow(0.85, slice.length - 1 - i);
    const w = recency * (0.5 + msg.classification.confidence);
    for (let c = 0; c < CATEGORY_COUNT; c += 1) {
      acc[c] += msg.classification.scores[c] * w;
    }
    weightSum += w;
  }

  const denom = Math.max(weightSum, 1e-9);
  for (let c = 0; c < CATEGORY_COUNT; c += 1) {
    acc[c] /= denom;
  }

  let bestIdx = 0;
  let bestScore = acc[0];
  for (let c = 1; c < CATEGORY_COUNT; c += 1) {
    if (acc[c] > bestScore) {
      bestScore = acc[c];
      bestIdx = c;
    }
  }

  return {
    category: CATEGORY_LIST[bestIdx],
    confidence: bestScore,
    scores: acc,
    reason: 'thread_consensus',
  };
}
