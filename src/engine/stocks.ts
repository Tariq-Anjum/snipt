/**
 * Offline Indian Stock Market Portfolio, Trade & Dividend Engine.
 *
 * Parses SEBI-mandated NSE/BSE/CDSL/NSDL & Indian broker SMS (Zerodha, Groww,
 * Upstox, Angel One, ICICI Direct, HDFC Sec) AND combines with manual trades to
 * compute Holdings, Weighted-Avg Cost, Unrealized P&L, Realized P&L, Dividends,
 * and estimated Indian STCG (20%) / LTCG (12.5% above Rs.1.25L exemption).
 */

export type TradeSide = 'BUY' | 'SELL' | 'DIVIDEND';
export type StockExchange = 'NSE' | 'BSE';

export interface StockTrade {
  id: string;
  symbol: string;
  companyName?: string;
  exchange: StockExchange;
  side: TradeSide;
  quantity: number;
  price: number; // per share for BUY/SELL; total payout for DIVIDEND
  fees: number; // STT, brokerage, GST
  timestampMs: number;
  broker?: string;
  note?: string;
  source: 'sms' | 'manual';
  sourceSmsId?: number;
  sourceBody?: string;
}

export interface HoldingSummary {
  symbol: string;
  companyName: string;
  exchange: StockExchange;
  quantity: number;
  avgBuyPrice: number;
  ltp: number;
  invested: number;
  currentValue: number;
  unrealizedPnl: number;
  unrealizedPct: number;
  dayChangePct: number;
  earliestBuyMs: number;
}

export interface RealizedTradeEvent {
  tradeId: string;
  symbol: string;
  quantity: number;
  sellPrice: number;
  costBasisPerShare: number;
  realizedPnl: number;
  isLongTerm: boolean; // >= 365 days holding period
  timestampMs: number;
}

export interface PortfolioSummary {
  holdings: HoldingSummary[];
  realizedEvents: RealizedTradeEvent[];
  totalInvested: number;
  totalCurrentValue: number;
  totalUnrealizedPnl: number;
  totalUnrealizedPct: number;
  totalRealizedPnl: number;
  totalDividends: number;
  netTotalPnl: number;
  stcgGains: number;
  ltcgGains: number;
  estimatedTax: number;
}

const KNOWN_COMPANIES: Record<string, string> = {
  RELIANCE: 'Reliance Industries Ltd',
  TCS: 'Tata Consultancy Services',
  HDFCBANK: 'HDFC Bank Ltd',
  INFY: 'Infosys Ltd',
  ICICIBANK: 'ICICI Bank Ltd',
  TATAMOTORS: 'Tata Motors Ltd',
  ITC: 'ITC Ltd',
  SBIN: 'State Bank of India',
  BHARTIARTL: 'Bharti Airtel Ltd',
  NIFTYBEES: 'Nippon India Nifty 50 BeES',
  ZOMATO: 'Eternal (Zomato) Ltd',
  BAJFINANCE: 'Bajaj Finance Ltd',
  LT: 'Larsen & Toubro Ltd',
};

export const DEFAULT_LTP_MAP: Record<string, number> = {
  RELIANCE: 1324.5,
  TCS: 4185.0,
  HDFCBANK: 1762.4,
  INFY: 1894.0,
  TATAMOTORS: 942.2,
  ITC: 486.8,
  NIFTYBEES: 274.6,
  ICICIBANK: 1298.5,
  SBIN: 842.0,
};

const TRADES_STORAGE_KEY = 'snipt-stock-trades-v1';
const LTP_STORAGE_KEY = 'snipt-stock-ltp-v1';

function parseNum(raw: string): number {
  const n = Number(raw.replace(/,/g, '').trim());
  return Number.isFinite(n) ? n : 0;
}

/**
 * Parse Indian broker / exchange / depository SMS alerts into a structured StockTrade.
 * Examples handled:
 * - "Order Executed: BOUGHT 15 qty of RELIANCE on NSE at avg price Rs. 1,265.00 via Zerodha."
 * - "NSE Alert: SOLD 10 shares of TCS @ Rs. 4,120.50 on 18-Nov-26. Broker: Groww."
 * - "CDSL: Your Demat A/c credited with 25 shares of HDFCBANK @ Rs. 1,690.00."
 * - "Dividend of Rs. 420.00 credited to your bank A/C for ITC (30 shares)."
 */
export function parseStockSms(
  sender: string,
  body: string,
  timestampMs: number,
  smsId?: number
): StockTrade | null {
  const text = body.trim();
  if (!text) return null;

  const upperSender = sender.toUpperCase();
  const brokerHint = /ZERODHA|KITE/i.test(text + upperSender)
    ? 'Zerodha'
    : /GROWW/i.test(text + upperSender)
      ? 'Groww'
      : /UPSTOX/i.test(text + upperSender)
        ? 'Upstox'
        : /ANGEL/i.test(text + upperSender)
          ? 'Angel One'
          : /CDSL/i.test(text + upperSender)
            ? 'CDSL'
            : /NSDL/i.test(text + upperSender)
              ? 'NSDL'
              : /NSE/i.test(text + upperSender)
                ? 'NSE Alert'
                : /BSE/i.test(text + upperSender)
                  ? 'BSE Alert'
                  : 'Broker SMS';

  const exchange: StockExchange = /\bBSE\b/i.test(text) ? 'BSE' : 'NSE';

  // 1. Dividend credit SMS
  const divMatch = text.match(
    /dividend(?:\s+of)?\s+(?:rs\.?|inr|₹)\s*([\d,]+(?:\.\d{1,2})?).*?(?:for|from|of)\s+([A-Z][A-Z0-9&]{1,14})\b/i
  );
  if (divMatch) {
    const payout = parseNum(divMatch[1]);
    const symbol = divMatch[2].toUpperCase();
    if (payout > 0) {
      return {
        id: `sms-div-${smsId ?? timestampMs}-${symbol}`,
        symbol,
        companyName: KNOWN_COMPANIES[symbol] ?? symbol,
        exchange,
        side: 'DIVIDEND',
        quantity: 1,
        price: payout,
        fees: 0,
        timestampMs,
        broker: brokerHint,
        note: 'Auto-parsed dividend SMS',
        source: 'sms',
        sourceSmsId: smsId,
        sourceBody: text,
      };
    }
  }

  // 2. Equity BUY / SELL / Demat credit / debit SMS
  const isBuy = /\b(bought|buy|purchased|demat.*credited|credited\s+with\s+\d+\s+shares?)\b/i.test(text);
  const isSell = /\b(sold|sell|demat.*debited|debited\s+with\s+\d+\s+shares?)\b/i.test(text);
  if (!isBuy && !isSell) return null;

  // Match pattern: "<qty> (qty|shares|units) of <SYMBOL>" or "<SYMBOL> <qty> qty"
  const qtySymA = text.match(
    /\b(\d+(?:\.\d+)?)\s*(?:qty|quantity|shares?|units?)\s*(?:of\s+)?([A-Z][A-Z0-9&-]{1,14})\b/i
  );
  const qtySymB = text.match(
    /\b(?:bought|sold|buy|sell)\s+([A-Z][A-Z0-9&-]{1,14})\s+(\d+(?:\.\d+)?)\s*(?:qty|shares?|units?)/i
  );

  let symbol = '';
  let quantity = 0;
  if (qtySymA) {
    quantity = parseNum(qtySymA[1]);
    symbol = qtySymA[2].toUpperCase();
  } else if (qtySymB) {
    symbol = qtySymB[1].toUpperCase();
    quantity = parseNum(qtySymB[2]);
  }
  if (!symbol || quantity <= 0) return null;
  if (['ON', 'AT', 'VIA', 'FOR', 'NSE', 'BSE', 'INR', 'RS'].includes(symbol)) return null;

  const priceMatch = text.match(
    /(?:@\s*|at\s+(?:avg\.?\s*)?(?:price\s*)?|price\s*(?:of\s*)?)(?:rs\.?|inr|₹)?\s*([\d,]+(?:\.\d{1,2})?)/i
  );
  const price = priceMatch ? parseNum(priceMatch[1]) : (DEFAULT_LTP_MAP[symbol] ?? 0);
  if (price <= 0) return null;

  return {
    id: `sms-trd-${smsId ?? timestampMs}-${symbol}`,
    symbol,
    companyName: KNOWN_COMPANIES[symbol] ?? symbol,
    exchange,
    side: isSell && !isBuy ? 'SELL' : 'BUY',
    quantity,
    price,
    fees: 0,
    timestampMs,
    broker: brokerHint,
    note: 'Auto-parsed from broker/exchange SMS',
    source: 'sms',
    sourceSmsId: smsId,
    sourceBody: text,
  };
}

export function createSeedStockTrades(nowMs: number = Date.now()): StockTrade[] {
  const day = 86_400_000;
  return [
    {
      id: 'st-1',
      symbol: 'RELIANCE',
      companyName: 'Reliance Industries Ltd',
      exchange: 'NSE',
      side: 'BUY',
      quantity: 25,
      price: 1218.0,
      fees: 48,
      timestampMs: nowMs - 410 * day,
      broker: 'Zerodha',
      note: 'Core long-term allocation',
      source: 'sms',
      sourceBody: 'NSE Alert: BOUGHT 25 shares of RELIANCE on NSE at avg price Rs. 1,218.00 via Zerodha.',
    },
    {
      id: 'st-2',
      symbol: 'TCS',
      companyName: 'Tata Consultancy Services',
      exchange: 'NSE',
      side: 'BUY',
      quantity: 12,
      price: 3890.0,
      fees: 62,
      timestampMs: nowMs - 240 * day,
      broker: 'Zerodha',
      note: 'IT bellwether SIP',
      source: 'sms',
      sourceBody: 'CDSL: Your Demat A/c credited with 12 shares of TCS @ Rs. 3,890.00.',
    },
    {
      id: 'st-3',
      symbol: 'HDFCBANK',
      companyName: 'HDFC Bank Ltd',
      exchange: 'NSE',
      side: 'BUY',
      quantity: 30,
      price: 1625.0,
      fees: 55,
      timestampMs: nowMs - 145 * day,
      broker: 'Groww',
      note: 'Banking dip buy',
      source: 'sms',
      sourceBody: 'Groww: Order Executed — BOUGHT 30 qty of HDFCBANK on NSE @ Rs. 1,625.00.',
    },
    {
      id: 'st-4',
      symbol: 'INFY',
      companyName: 'Infosys Ltd',
      exchange: 'NSE',
      side: 'BUY',
      quantity: 20,
      price: 1710.0,
      fees: 44,
      timestampMs: nowMs - 95 * day,
      broker: 'Zerodha',
      note: 'Earnings breakout',
      source: 'manual',
    },
    {
      id: 'st-5',
      symbol: 'TCS',
      companyName: 'Tata Consultancy Services',
      exchange: 'NSE',
      side: 'SELL',
      quantity: 4,
      price: 4160.0,
      fees: 32,
      timestampMs: nowMs - 18 * day,
      broker: 'Zerodha',
      note: 'Partial profit booking',
      source: 'sms',
      sourceBody: 'NSE Alert: SOLD 4 shares of TCS @ Rs. 4,160.00 via Zerodha.',
    },
    {
      id: 'st-6',
      symbol: 'NIFTYBEES',
      companyName: 'Nippon India Nifty 50 BeES',
      exchange: 'NSE',
      side: 'BUY',
      quantity: 120,
      price: 256.5,
      fees: 24,
      timestampMs: nowMs - 60 * day,
      broker: 'Groww',
      note: 'Index ETF monthly SIP',
      source: 'manual',
    },
    {
      id: 'st-7',
      symbol: 'ITC',
      companyName: 'ITC Ltd',
      exchange: 'NSE',
      side: 'BUY',
      quantity: 60,
      price: 452.0,
      fees: 28,
      timestampMs: nowMs - 400 * day,
      broker: 'Zerodha',
      note: 'Dividend compounder',
      source: 'sms',
      sourceBody: 'CDSL: Your Demat A/c credited with 60 shares of ITC @ Rs. 452.00.',
    },
    {
      id: 'st-8',
      symbol: 'ITC',
      companyName: 'ITC Ltd',
      exchange: 'NSE',
      side: 'DIVIDEND',
      quantity: 60,
      price: 450.0,
      fees: 0,
      timestampMs: nowMs - 32 * day,
      broker: 'Bank Dividend',
      note: 'Final dividend ₹7.50/share',
      source: 'sms',
      sourceBody: 'Dividend of Rs. 450.00 credited to your bank A/C for ITC (60 shares).',
    },
    {
      id: 'st-9',
      symbol: 'RELIANCE',
      companyName: 'Reliance Industries Ltd',
      exchange: 'NSE',
      side: 'SELL',
      quantity: 5,
      price: 1335.0,
      fees: 19,
      timestampMs: nowMs - 7 * day,
      broker: 'Zerodha',
      note: 'LTCG harvest',
      source: 'manual',
    },
  ];
}

export function loadStockTrades(nowMs: number = Date.now()): StockTrade[] {
  try {
    const raw = window.localStorage.getItem(TRADES_STORAGE_KEY);
    if (raw) {
      const parsed = JSON.parse(raw) as StockTrade[];
      if (Array.isArray(parsed) && parsed.length > 0) return parsed;
    }
  } catch {
    // ignore
  }
  return createSeedStockTrades(nowMs);
}

export function saveStockTrades(trades: StockTrade[]): void {
  try {
    window.localStorage.setItem(TRADES_STORAGE_KEY, JSON.stringify(trades));
  } catch {
    // ignore
  }
}

export function loadLtpMap(): Record<string, number> {
  try {
    const raw = window.localStorage.getItem(LTP_STORAGE_KEY);
    if (raw) {
      return { ...DEFAULT_LTP_MAP, ...(JSON.parse(raw) as Record<string, number>) };
    }
  } catch {
    // ignore
  }
  return { ...DEFAULT_LTP_MAP };
}

export function saveLtpMap(map: Record<string, number>): void {
  try {
    window.localStorage.setItem(LTP_STORAGE_KEY, JSON.stringify(map));
  } catch {
    // ignore
  }
}

const ONE_YEAR_MS = 365 * 86_400_000;

/** Compute FIFO holdings, unrealized/realized P&L, dividends, and Indian STCG/LTCG tax estimate. */
export function computeStockPortfolio(
  trades: StockTrade[],
  ltpMap: Record<string, number>
): PortfolioSummary {
  const sorted = [...trades].sort((a, b) => a.timestampMs - b.timestampMs);

  interface Lot {
    qty: number;
    costPerShare: number;
    timestampMs: number;
  }

  const lotsBySymbol = new Map<
    string,
    { companyName: string; exchange: StockExchange; lots: Lot[] }
  >();
  const realizedEvents: RealizedTradeEvent[] = [];
  let totalDividends = 0;

  for (const t of sorted) {
    const sym = t.symbol.toUpperCase();
    if (t.side === 'DIVIDEND') {
      totalDividends += t.price;
      continue;
    }

    let entry = lotsBySymbol.get(sym);
    if (!entry) {
      entry = {
        companyName: t.companyName || KNOWN_COMPANIES[sym] || sym,
        exchange: t.exchange,
        lots: [],
      };
      lotsBySymbol.set(sym, entry);
    }

    if (t.side === 'BUY') {
      const effectiveCost = (t.quantity * t.price + t.fees) / Math.max(t.quantity, 1e-9);
      entry.lots.push({
        qty: t.quantity,
        costPerShare: effectiveCost,
        timestampMs: t.timestampMs,
      });
    } else if (t.side === 'SELL') {
      let remaining = t.quantity;
      let totalCostMatched = 0;
      let matchedQty = 0;
      let longTermQty = 0;

      while (remaining > 1e-9 && entry.lots.length > 0) {
        const lot = entry.lots[0];
        const take = Math.min(remaining, lot.qty);
        totalCostMatched += take * lot.costPerShare;
        matchedQty += take;
        if (t.timestampMs - lot.timestampMs >= ONE_YEAR_MS) {
          longTermQty += take;
        }
        lot.qty -= take;
        remaining -= take;
        if (lot.qty <= 1e-9) {
          entry.lots.shift();
        }
      }

      if (matchedQty > 0) {
        const proceeds = matchedQty * t.price - t.fees;
        const realizedPnl = proceeds - totalCostMatched;
        realizedEvents.push({
          tradeId: t.id,
          symbol: sym,
          quantity: matchedQty,
          sellPrice: t.price,
          costBasisPerShare: totalCostMatched / matchedQty,
          realizedPnl,
          isLongTerm: longTermQty >= matchedQty / 2,
          timestampMs: t.timestampMs,
        });
      }
    }
  }

  const holdings: HoldingSummary[] = [];
  for (const [symbol, info] of lotsBySymbol.entries()) {
    const totalQty = info.lots.reduce((s, l) => s + l.qty, 0);
    if (totalQty <= 1e-6) continue;
    const invested = info.lots.reduce((s, l) => s + l.qty * l.costPerShare, 0);
    const avgBuyPrice = invested / totalQty;
    const ltp = ltpMap[symbol] ?? avgBuyPrice;
    const currentValue = totalQty * ltp;
    const unrealizedPnl = currentValue - invested;
    const unrealizedPct = invested > 0 ? (unrealizedPnl / invested) * 100 : 0;

    holdings.push({
      symbol,
      companyName: info.companyName,
      exchange: info.exchange,
      quantity: Number(totalQty.toFixed(4)),
      avgBuyPrice,
      ltp,
      invested,
      currentValue,
      unrealizedPnl,
      unrealizedPct,
      dayChangePct: 0,
      earliestBuyMs: info.lots[0]?.timestampMs ?? Date.now(),
    });
  }

  holdings.sort((a, b) => b.currentValue - a.currentValue);

  const totalInvested = holdings.reduce((s, h) => s + h.invested, 0);
  const totalCurrentValue = holdings.reduce((s, h) => s + h.currentValue, 0);
  const totalUnrealizedPnl = totalCurrentValue - totalInvested;
  const totalUnrealizedPct = totalInvested > 0 ? (totalUnrealizedPnl / totalInvested) * 100 : 0;
  const totalRealizedPnl = realizedEvents.reduce((s, e) => s + e.realizedPnl, 0);

  const stcgGains = realizedEvents
    .filter((e) => !e.isLongTerm)
    .reduce((s, e) => s + e.realizedPnl, 0);
  const ltcgGains = realizedEvents
    .filter((e) => e.isLongTerm)
    .reduce((s, e) => s + e.realizedPnl, 0);

  // Indian FY25-26 equity taxation estimate: STCG 20%, LTCG 12.5% above Rs. 1,25,000 exemption
  const taxableStcg = Math.max(0, stcgGains);
  const taxableLtcg = Math.max(0, ltcgGains - 125_000);
  const estimatedTax = taxableStcg * 0.2 + taxableLtcg * 0.125;

  return {
    holdings,
    realizedEvents: realizedEvents.reverse(),
    totalInvested,
    totalCurrentValue,
    totalUnrealizedPnl,
    totalUnrealizedPct,
    totalRealizedPnl,
    totalDividends,
    netTotalPnl: totalUnrealizedPnl + totalRealizedPnl + totalDividends,
    stcgGains,
    ltcgGains,
    estimatedTax,
  };
}
