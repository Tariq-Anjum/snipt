import {
  ArrowDownLeft,
  ArrowUpRight,
  FileText,
  Plus,
  Sparkles,
  Trash2,
  TrendingDown,
  TrendingUp,
  X,
} from 'lucide-react';
import { useMemo, useState, type FormEvent } from 'react';
import {
  computeStockPortfolio,
  parseStockSms,
  type StockExchange,
  type StockTrade,
  type TradeSide,
} from '../engine/stocks';
import { formatFullDateTime, formatInr } from '../utils/format';

interface StocksViewProps {
  trades: StockTrade[];
  ltpMap: Record<string, number>;
  onAddTrade: (trade: StockTrade) => void;
  onDeleteTrade: (id: string) => void;
  onUpdateLtp: (symbol: string, ltp: number) => void;
}

const SAMPLE_BROKER_SMS = [
  {
    label: 'Zerodha Buy',
    sender: 'VM-ZERODH-S',
    body: 'Order Executed: BOUGHT 10 shares of INFY on NSE at avg price Rs. 1,875.00 via Zerodha.',
  },
  {
    label: 'Groww Sell',
    sender: 'AX-GROWW-S',
    body: 'NSE Alert: SOLD 5 shares of HDFCBANK @ Rs. 1,780.00 via Groww.',
  },
  {
    label: 'Dividend SMS',
    sender: 'JD-HDFCBK-S',
    body: 'Dividend of Rs. 680.00 credited to your bank A/C for RELIANCE (20 shares).',
  },
];

export function StocksView({
  trades,
  ltpMap,
  onAddTrade,
  onDeleteTrade,
  onUpdateLtp,
}: StocksViewProps) {
  const [subTab, setSubTab] = useState<'holdings' | 'ledger'>('holdings');
  const [modalOpen, setModalOpen] = useState(false);
  const [smsOpen, setSmsOpen] = useState(false);

  // Manual trade form state
  const [side, setSide] = useState<TradeSide>('BUY');
  const [symbol, setSymbol] = useState('');
  const [exchange, setExchange] = useState<StockExchange>('NSE');
  const [qtyStr, setQtyStr] = useState('');
  const [priceStr, setPriceStr] = useState('');
  const [feesStr, setFeesStr] = useState('25');
  const [broker, setBroker] = useState('Zerodha');
  const [note, setNote] = useState('');
  const [dateIso, setDateIso] = useState(() => new Date().toISOString().slice(0, 10));

  // SMS quick parser state
  const [smsInput, setSmsInput] = useState('');
  const [smsFeedback, setSmsFeedback] = useState<string | null>(null);

  const portfolio = useMemo(() => computeStockPortfolio(trades, ltpMap), [trades, ltpMap]);

  const sortedTrades = useMemo(
    () => [...trades].sort((a, b) => b.timestampMs - a.timestampMs),
    [trades]
  );

  const realizedByTradeId = useMemo(() => {
    const m = new Map<string, { pnl: number; isLongTerm: boolean }>();
    for (const ev of portfolio.realizedEvents) {
      m.set(ev.tradeId, { pnl: ev.realizedPnl, isLongTerm: ev.isLongTerm });
    }
    return m;
  }, [portfolio.realizedEvents]);

  const openPresetTrade = (sym: string, presetSide: TradeSide, refPrice: number) => {
    setSymbol(sym);
    setSide(presetSide);
    setPriceStr(String(refPrice));
    setQtyStr('5');
    setModalOpen(true);
  };

  const handleCreateTrade = (e: FormEvent) => {
    e.preventDefault();
    const sym = symbol.trim().toUpperCase();
    const qty = side === 'DIVIDEND' ? 1 : Number(qtyStr);
    const price = Number(priceStr);
    const fees = Number(feesStr) || 0;
    if (!sym || !Number.isFinite(qty) || qty <= 0 || !Number.isFinite(price) || price <= 0) return;

    const ts = Date.parse(`${dateIso}T10:30:00`) || Date.now();
    onAddTrade({
      id: `trd-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
      symbol: sym,
      exchange,
      side,
      quantity: qty,
      price,
      fees: side === 'DIVIDEND' ? 0 : fees,
      timestampMs: ts,
      broker: broker.trim() || 'Manual',
      note: note.trim() || undefined,
      source: 'manual',
    });
    if (side !== 'DIVIDEND' && !ltpMap[sym]) {
      onUpdateLtp(sym, price);
    }
    setSymbol('');
    setQtyStr('');
    setPriceStr('');
    setNote('');
    setModalOpen(false);
  };

  const handleParseSms = (e: FormEvent) => {
    e.preventDefault();
    const parsed = parseStockSms('BROKER-SMS', smsInput, Date.now());
    if (!parsed) {
      setSmsFeedback(
        'Could not detect a stock Buy/Sell/Dividend trade. Try one of the sample NSE/CDSL alerts below.'
      );
      return;
    }
    onAddTrade(parsed);
    if (parsed.side !== 'DIVIDEND' && !ltpMap[parsed.symbol]) {
      onUpdateLtp(parsed.symbol, parsed.price);
    }
    setSmsInput('');
    setSmsFeedback(
      `Imported ${parsed.side} ${parsed.quantity} ${parsed.symbol} @ ${formatInr(parsed.price)}.`
    );
  };

  return (
    <main className="flex-1 overflow-y-auto smooth-scroll-layer bg-slate-50 dark:bg-slate-950 glass:bg-transparent px-4 md:px-8 py-6 pb-24 md:pb-10">
      <div className="max-w-7xl mx-auto space-y-6">
        {/* Header */}
        <div className="flex flex-col sm:flex-row sm:items-end justify-between gap-4">
          <div>
            <h1 className="font-display text-2xl font-bold tracking-tight text-slate-900 dark:text-slate-100">
              Stocks, Trades &amp; Realized P&amp;L
            </h1>
            <p className="mt-1 text-sm text-slate-500 dark:text-slate-400">
              Offline NSE/BSE portfolio tracker auto-fed from Zerodha, Groww, CDSL &amp; dividend SMS with FIFO profit &amp; tax estimates.
            </p>
          </div>
          <div className="flex items-center gap-2 self-start sm:self-auto">
            <button
              type="button"
              onClick={() => {
                setSmsFeedback(null);
                setSmsOpen((v) => !v);
              }}
              className="inline-flex items-center gap-1.5 px-3.5 py-2 text-xs font-semibold text-slate-700 dark:text-slate-200 bg-white dark:bg-slate-900 glass-panel border border-slate-200 dark:border-slate-800 rounded-xl hover:bg-slate-50 dark:hover:bg-slate-800 transition-colors whitespace-nowrap"
            >
              <FileText className="w-3.5 h-3.5" aria-hidden="true" />
              <span>Parse Broker SMS</span>
            </button>
            <button
              type="button"
              onClick={() => setModalOpen(true)}
              className="inline-flex items-center gap-1.5 px-4 py-2 text-xs font-semibold text-white snipt-accent-bg snipt-accent-on-solid glass-cta rounded-xl transition-colors whitespace-nowrap"
            >
              <Plus className="w-3.5 h-3.5" aria-hidden="true" />
              <span>Record Trade</span>
            </button>
          </div>
        </div>

        {/* Inline Broker SMS Parser Drawer */}
        {smsOpen && (
          <section className="bg-white dark:bg-slate-900 glass-panel border border-slate-200 dark:border-slate-800 rounded-3xl p-5 space-y-3">
            <div className="flex items-center justify-between gap-2">
              <div>
                <h2 className="font-display text-sm font-semibold text-slate-900 dark:text-slate-100">
                  Import from NSE / BSE / CDSL / Broker SMS
                </h2>
                <p className="text-xs text-slate-500 dark:text-slate-400">
                  Paste any Indian equity execution, demat credit, or dividend SMS to parse offline.
                </p>
              </div>
              <button
                type="button"
                onClick={() => setSmsOpen(false)}
                aria-label="Close SMS parser"
                className="p-1.5 rounded-lg text-slate-400 hover:text-slate-700 dark:hover:text-slate-200"
              >
                <X className="w-4 h-4" aria-hidden="true" />
              </button>
            </div>
            <div className="flex flex-wrap items-center gap-1.5">
              {SAMPLE_BROKER_SMS.map((sample) => (
                <button
                  key={sample.label}
                  type="button"
                  onClick={() => setSmsInput(sample.body)}
                  className="px-2.5 py-1 text-xs font-medium rounded-lg bg-slate-100 hover:bg-slate-200 dark:bg-slate-800 dark:hover:bg-slate-700 text-slate-700 dark:text-slate-200 transition-colors"
                >
                  {sample.label}
                </button>
              ))}
            </div>
            <form onSubmit={handleParseSms} className="flex flex-col sm:flex-row gap-2">
              <input
                type="text"
                value={smsInput}
                onChange={(e) => setSmsInput(e.target.value)}
                placeholder="Paste Zerodha / Groww / CDSL / NSE buy, sell, or dividend SMS..."
                className="flex-1 px-3.5 py-2.5 text-sm rounded-2xl border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 text-slate-900 dark:text-slate-100"
              />
              <button
                type="submit"
                disabled={!smsInput.trim()}
                className="px-4 py-2.5 text-xs font-semibold text-white snipt-accent-bg snipt-accent-on-solid rounded-2xl disabled:opacity-40 whitespace-nowrap"
              >
                Parse &amp; Add
              </button>
            </form>
            {smsFeedback && (
              <p className="text-xs font-medium text-blue-700 dark:text-blue-400">{smsFeedback}</p>
            )}
          </section>
        )}

        {/* 4-column Portfolio KPI strip */}
        <section
          aria-label="Stock portfolio summary"
          className="grid grid-cols-2 lg:grid-cols-4 gap-4"
        >
          <div className="bg-white dark:bg-slate-900 glass-panel border border-slate-200 dark:border-slate-800 rounded-3xl p-5">
            <p className="text-xs font-medium text-slate-500 dark:text-slate-400">Current Value</p>
            <p className="mt-1.5 font-mono text-2xl font-bold tabular-nums text-slate-900 dark:text-slate-100">
              {formatInr(portfolio.totalCurrentValue)}
            </p>
            <p className="mt-1 text-xs text-slate-500 dark:text-slate-400">
              Invested {formatInr(portfolio.totalInvested)}
            </p>
          </div>

          <div className="bg-white dark:bg-slate-900 glass-panel border border-slate-200 dark:border-slate-800 rounded-3xl p-5">
            <p className="text-xs font-medium text-slate-500 dark:text-slate-400">Unrealized P&amp;L</p>
            <p
              className={`mt-1.5 font-mono text-2xl font-bold tabular-nums ${
                portfolio.totalUnrealizedPnl >= 0
                  ? 'text-emerald-700 dark:text-emerald-400'
                  : 'text-red-700 dark:text-red-400'
              }`}
            >
              {portfolio.totalUnrealizedPnl >= 0 ? '+' : ''}
              {formatInr(portfolio.totalUnrealizedPnl)}
            </p>
            <p className="mt-1 font-mono text-xs tabular-nums text-slate-500 dark:text-slate-400">
              {portfolio.totalUnrealizedPct >= 0 ? '+' : ''}
              {portfolio.totalUnrealizedPct.toFixed(2)}% overall return
            </p>
          </div>

          <div className="bg-white dark:bg-slate-900 glass-panel border border-slate-200 dark:border-slate-800 rounded-3xl p-5">
            <p className="text-xs font-medium text-slate-500 dark:text-slate-400">Realized Profit</p>
            <p
              className={`mt-1.5 font-mono text-2xl font-bold tabular-nums ${
                portfolio.totalRealizedPnl >= 0
                  ? 'text-emerald-700 dark:text-emerald-400'
                  : 'text-red-700 dark:text-red-400'
              }`}
            >
              {portfolio.totalRealizedPnl >= 0 ? '+' : ''}
              {formatInr(portfolio.totalRealizedPnl)}
            </p>
            <p className="mt-1 text-xs text-slate-500 dark:text-slate-400">
              {portfolio.realizedEvents.length} closed sale{portfolio.realizedEvents.length === 1 ? '' : 's'} (FIFO)
            </p>
          </div>

          <div className="bg-white dark:bg-slate-900 glass-panel border border-slate-200 dark:border-slate-800 rounded-3xl p-5">
            <p className="text-xs font-medium text-slate-500 dark:text-slate-400">
              Dividends &amp; Est. Tax
            </p>
            <p className="mt-1.5 font-mono text-2xl font-bold tabular-nums text-blue-700 dark:text-blue-400">
              +{formatInr(portfolio.totalDividends)}
            </p>
            <p className="mt-1 font-mono text-xs tabular-nums text-slate-500 dark:text-slate-400">
              Est. Cap Gains Tax: {formatInr(portfolio.estimatedTax)}
            </p>
          </div>
        </section>

        {/* Indian Capital Gains Breakdown + Allocation Strip */}
        <section className="grid grid-cols-1 lg:grid-cols-3 gap-6">
          <div className="lg:col-span-2 bg-white dark:bg-slate-900 glass-panel border border-slate-200 dark:border-slate-800 rounded-3xl p-5 space-y-4">
            <div className="flex items-center justify-between">
              <h2 className="font-display text-base font-semibold text-slate-900 dark:text-slate-100">
                Portfolio Weight Distribution
              </h2>
              <span className="font-mono text-xs text-slate-500 dark:text-slate-400">
                {portfolio.holdings.length} active positions
              </span>
            </div>
            <div className="space-y-3">
              {portfolio.holdings.slice(0, 6).map((h) => {
                const weight =
                  portfolio.totalCurrentValue > 0
                    ? (h.currentValue / portfolio.totalCurrentValue) * 100
                    : 0;
                const positive = h.unrealizedPnl >= 0;
                return (
                  <div key={h.symbol} className="space-y-1">
                    <div className="flex items-center justify-between text-xs">
                      <span className="font-semibold text-slate-900 dark:text-slate-100">
                        {h.symbol}{' '}
                        <span className="font-normal text-slate-500 dark:text-slate-400">
                          · {h.quantity} qty
                        </span>
                      </span>
                      <span className="font-mono tabular-nums text-slate-700 dark:text-slate-300">
                        {formatInr(h.currentValue)} ({weight.toFixed(1)}%)
                      </span>
                    </div>
                    <div className="h-2 rounded-full bg-slate-100 dark:bg-slate-800 overflow-hidden">
                      <div
                        className={`h-full rounded-full ${
                          positive
                            ? 'bg-emerald-600 dark:bg-emerald-500'
                            : 'bg-red-500 dark:bg-red-400'
                        }`}
                        style={{ width: `${Math.max(4, weight)}%` }}
                      />
                    </div>
                  </div>
                );
              })}
            </div>
          </div>

          <div className="bg-white dark:bg-slate-900 glass-panel border border-slate-200 dark:border-slate-800 rounded-3xl p-5 flex flex-col justify-between space-y-4">
            <div>
              <h2 className="font-display text-base font-semibold text-slate-900 dark:text-slate-100">
                Indian Equity Tax Snapshot
              </h2>
              <p className="mt-1 text-xs text-slate-500 dark:text-slate-400">
                FIFO realized gains estimate (FY rules: STCG 20%, LTCG 12.5% above ₹1.25L exemption).
              </p>
            </div>
            <div className="space-y-2.5 text-xs">
              <div className="flex items-center justify-between py-2 border-b border-slate-100 dark:border-slate-800">
                <span className="text-slate-600 dark:text-slate-400">STCG (&lt; 1 year)</span>
                <span className="font-mono font-semibold tabular-nums text-slate-900 dark:text-slate-100">
                  {portfolio.stcgGains >= 0 ? '+' : ''}
                  {formatInr(portfolio.stcgGains)}
                </span>
              </div>
              <div className="flex items-center justify-between py-2 border-b border-slate-100 dark:border-slate-800">
                <span className="text-slate-600 dark:text-slate-400">LTCG (&ge; 1 year)</span>
                <span className="font-mono font-semibold tabular-nums text-slate-900 dark:text-slate-100">
                  {portfolio.ltcgGains >= 0 ? '+' : ''}
                  {formatInr(portfolio.ltcgGains)}
                </span>
              </div>
              <div className="flex items-center justify-between py-2">
                <span className="font-semibold text-slate-900 dark:text-slate-100">
                  Estimated Tax Liability
                </span>
                <span className="font-mono font-bold tabular-nums text-blue-700 dark:text-blue-400">
                  {formatInr(portfolio.estimatedTax)}
                </span>
              </div>
            </div>
            <p className="text-[11px] text-slate-400">
              100% offline: edit any LTP below to refresh current portfolio value without internet.
            </p>
          </div>
        </section>

        {/* Sub-view Switcher: Holdings vs Trade Ledger */}
        <section className="bg-white dark:bg-slate-900 glass-panel border border-slate-200 dark:border-slate-800 rounded-3xl overflow-hidden">
          <div className="p-4 sm:p-5 border-b border-slate-200/80 dark:border-slate-800 flex flex-wrap items-center justify-between gap-3">
            <div className="inline-flex items-center gap-1 p-1 rounded-xl bg-slate-100 dark:bg-slate-800">
              <button
                type="button"
                onClick={() => setSubTab('holdings')}
                className={`px-3.5 py-1.5 text-xs font-semibold rounded-lg transition-colors ${
                  subTab === 'holdings'
                    ? 'bg-white dark:bg-slate-900 text-slate-900 dark:text-white shadow-xs'
                    : 'text-slate-500 dark:text-slate-400'
                }`}
              >
                Holdings ({portfolio.holdings.length})
              </button>
              <button
                type="button"
                onClick={() => setSubTab('ledger')}
                className={`px-3.5 py-1.5 text-xs font-semibold rounded-lg transition-colors ${
                  subTab === 'ledger'
                    ? 'bg-white dark:bg-slate-900 text-slate-900 dark:text-white shadow-xs'
                    : 'text-slate-500 dark:text-slate-400'
                }`}
              >
                Buy / Sell &amp; Dividends ({sortedTrades.length})
              </button>
            </div>
            <p className="text-xs text-slate-500 dark:text-slate-400">
              Net Total Return (Unrealized + Realized + Dividends):{' '}
              <span
                className={`font-mono font-semibold tabular-nums ${
                  portfolio.netTotalPnl >= 0
                    ? 'text-emerald-700 dark:text-emerald-400'
                    : 'text-red-700 dark:text-red-400'
                }`}
              >
                {portfolio.netTotalPnl >= 0 ? '+' : ''}
                {formatInr(portfolio.netTotalPnl)}
              </span>
            </p>
          </div>

          {subTab === 'holdings' ? (
            <div className="overflow-x-auto">
              <table className="w-full text-left border-collapse">
                <thead>
                  <tr className="border-b border-slate-100 dark:border-slate-800 text-[11px] font-mono uppercase tracking-wider text-slate-500 dark:text-slate-400">
                    <th className="py-3 px-5">Stock</th>
                    <th className="py-3 px-3 text-right">Qty</th>
                    <th className="py-3 px-3 text-right">Avg Buy</th>
                    <th className="py-3 px-3 text-right">LTP (Editable)</th>
                    <th className="py-3 px-3 text-right">Current Value</th>
                    <th className="py-3 px-3 text-right">Unrealized P&amp;L</th>
                    <th className="py-3 px-5 text-right">Action</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-slate-100 dark:divide-slate-800/80 text-sm">
                  {portfolio.holdings.map((h) => {
                    const positive = h.unrealizedPnl >= 0;
                    return (
                      <tr
                        key={h.symbol}
                        className="hover:bg-slate-50/70 dark:hover:bg-slate-800/30 transition-colors"
                      >
                        <td className="py-3.5 px-5">
                          <p className="font-semibold text-slate-900 dark:text-slate-100">
                            {h.symbol}
                          </p>
                          <p className="text-xs text-slate-500 dark:text-slate-400">
                            {h.companyName} · {h.exchange}
                          </p>
                        </td>
                        <td className="py-3.5 px-3 text-right font-mono tabular-nums text-slate-900 dark:text-slate-100">
                          {h.quantity}
                        </td>
                        <td className="py-3.5 px-3 text-right font-mono tabular-nums text-slate-600 dark:text-slate-300">
                          {formatInr(h.avgBuyPrice)}
                        </td>
                        <td className="py-3.5 px-3 text-right">
                          <input
                            type="number"
                            step="any"
                            min="0"
                            aria-label={`Latest traded price for ${h.symbol}`}
                            value={h.ltp}
                            onChange={(e) => {
                              const n = Number(e.target.value);
                              if (Number.isFinite(n) && n >= 0) onUpdateLtp(h.symbol, n);
                            }}
                            className="w-24 text-right font-mono text-xs tabular-nums px-2 py-1 rounded-lg border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 text-slate-900 dark:text-slate-100"
                          />
                        </td>
                        <td className="py-3.5 px-3 text-right font-mono font-semibold tabular-nums text-slate-900 dark:text-slate-100">
                          {formatInr(h.currentValue)}
                        </td>
                        <td className="py-3.5 px-3 text-right font-mono tabular-nums">
                          <span
                            className={`inline-flex items-center justify-end gap-1 font-semibold ${
                              positive
                                ? 'text-emerald-700 dark:text-emerald-400'
                                : 'text-red-700 dark:text-red-400'
                            }`}
                          >
                            {positive ? (
                              <TrendingUp className="w-3.5 h-3.5" aria-hidden="true" />
                            ) : (
                              <TrendingDown className="w-3.5 h-3.5" aria-hidden="true" />
                            )}
                            {positive ? '+' : ''}
                            {formatInr(h.unrealizedPnl)}
                          </span>
                          <p className="text-[11px] text-slate-500 dark:text-slate-400">
                            {positive ? '+' : ''}
                            {h.unrealizedPct.toFixed(2)}%
                          </p>
                        </td>
                        <td className="py-3.5 px-5 text-right whitespace-nowrap">
                          <div className="inline-flex items-center gap-1.5">
                            <button
                              type="button"
                              onClick={() => openPresetTrade(h.symbol, 'BUY', h.ltp)}
                              className="px-2.5 py-1 text-xs font-semibold text-emerald-700 dark:text-emerald-300 bg-emerald-50 dark:bg-emerald-950/50 hover:bg-emerald-100 dark:hover:bg-emerald-900/60 rounded-lg"
                            >
                              Buy
                            </button>
                            <button
                              type="button"
                              onClick={() => openPresetTrade(h.symbol, 'SELL', h.ltp)}
                              className="px-2.5 py-1 text-xs font-semibold text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/50 hover:bg-red-100 dark:hover:bg-red-900/60 rounded-lg"
                            >
                              Sell
                            </button>
                          </div>
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          ) : (
            <div className="divide-y divide-slate-100 dark:divide-slate-800/80">
              {sortedTrades.map((t) => {
                const realized = realizedByTradeId.get(t.id);
                const gross = t.side === 'DIVIDEND' ? t.price : t.quantity * t.price;
                return (
                  <div
                    key={t.id}
                    className="px-5 py-3.5 flex flex-col sm:flex-row sm:items-center justify-between gap-3 hover:bg-slate-50/70 dark:hover:bg-slate-800/30 transition-colors"
                  >
                    <div className="flex items-start gap-3 min-w-0">
                      <span
                        className={`w-9 h-9 rounded-2xl flex items-center justify-center shrink-0 ${
                          t.side === 'BUY'
                            ? 'bg-blue-50 text-blue-700 dark:bg-blue-950/60 dark:text-blue-300'
                            : t.side === 'SELL'
                              ? 'bg-emerald-50 text-emerald-700 dark:bg-emerald-950/60 dark:text-emerald-300'
                              : 'bg-amber-50 text-amber-700 dark:bg-amber-950/60 dark:text-amber-300'
                        }`}
                      >
                        {t.side === 'BUY' ? (
                          <ArrowDownLeft className="w-4 h-4" aria-hidden="true" />
                        ) : t.side === 'SELL' ? (
                          <ArrowUpRight className="w-4 h-4" aria-hidden="true" />
                        ) : (
                          <Sparkles className="w-4 h-4" aria-hidden="true" />
                        )}
                      </span>
                      <div className="min-w-0">
                        <p className="text-sm font-semibold text-slate-900 dark:text-slate-100">
                          {t.side} · {t.symbol}{' '}
                          {t.side !== 'DIVIDEND' && (
                            <span className="font-mono font-normal text-xs text-slate-500 dark:text-slate-400">
                              {t.quantity} qty @ {formatInr(t.price)}
                            </span>
                          )}
                        </p>
                        <p className="text-xs text-slate-500 dark:text-slate-400">
                          {formatFullDateTime(t.timestampMs)} · {t.broker || t.exchange} ·{' '}
                          {t.source === 'sms' ? 'Auto-parsed SMS' : 'Manual entry'}
                          {t.note ? ` · ${t.note}` : ''}
                        </p>
                      </div>
                    </div>

                    <div className="flex items-center justify-between sm:justify-end gap-4 shrink-0">
                      <div className="text-right font-mono tabular-nums">
                        <p className="text-sm font-semibold text-slate-900 dark:text-slate-100">
                          {formatInr(gross)}
                        </p>
                        {realized && (
                          <p
                            className={`text-xs font-semibold ${
                              realized.pnl >= 0
                                ? 'text-emerald-700 dark:text-emerald-400'
                                : 'text-red-700 dark:text-red-400'
                            }`}
                          >
                            {realized.pnl >= 0 ? 'Profit +' : 'Loss '}
                            {formatInr(realized.pnl)} ({realized.isLongTerm ? 'LTCG' : 'STCG'})
                          </p>
                        )}
                      </div>
                      <button
                        type="button"
                        onClick={() => onDeleteTrade(t.id)}
                        aria-label={`Delete ${t.symbol} trade`}
                        className="p-2 text-slate-400 hover:text-red-600 dark:hover:text-red-400 rounded-lg"
                      >
                        <Trash2 className="w-4 h-4" aria-hidden="true" />
                      </button>
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </section>
      </div>

      {/* Record Buy / Sell / Dividend Modal */}
      {modalOpen && (
        <div
          className="fixed inset-0 z-[60] flex items-end md:items-center justify-center p-0 md:p-4"
          role="presentation"
        >
          <button
            type="button"
            aria-label="Close record trade modal"
            onClick={() => setModalOpen(false)}
            className="absolute inset-0 bg-black/50"
          />
          <div
            role="dialog"
            aria-modal="true"
            aria-label="Record Stock Trade"
            className="sheet-enter relative w-full md:max-w-md rounded-t-3xl md:rounded-3xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 glass-sheet shadow-2xl p-5 space-y-4"
          >
            <div className="flex items-center justify-between">
              <h2 className="font-display text-lg font-bold text-slate-900 dark:text-slate-100">
                Record Stock Transaction
              </h2>
              <button
                type="button"
                onClick={() => setModalOpen(false)}
                aria-label="Close"
                className="p-2 rounded-full text-slate-500 hover:bg-slate-100 dark:hover:bg-slate-800"
              >
                <X className="w-5 h-5" aria-hidden="true" />
              </button>
            </div>

            <form onSubmit={handleCreateTrade} className="space-y-3.5">
              <div className="grid grid-cols-3 gap-1 p-1 rounded-xl bg-slate-100 dark:bg-slate-800">
                {(['BUY', 'SELL', 'DIVIDEND'] as const).map((s) => (
                  <button
                    key={s}
                    type="button"
                    onClick={() => setSide(s)}
                    className={`py-2 text-xs font-semibold rounded-lg transition-colors ${
                      side === s
                        ? 'bg-white dark:bg-slate-900 text-slate-900 dark:text-white shadow-xs'
                        : 'text-slate-500 dark:text-slate-400'
                    }`}
                  >
                    {s}
                  </button>
                ))}
              </div>

              <div className="grid grid-cols-3 gap-2.5">
                <div className="col-span-2">
                  <label className="block text-xs font-semibold text-slate-600 dark:text-slate-400 mb-1">
                    Stock Symbol
                  </label>
                  <input
                    type="text"
                    required
                    value={symbol}
                    onChange={(e) => setSymbol(e.target.value.toUpperCase())}
                    placeholder="e.g. RELIANCE, TCS"
                    className="w-full px-3 py-2 text-sm font-mono uppercase rounded-xl border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 text-slate-900 dark:text-slate-100"
                  />
                </div>
                <div>
                  <label className="block text-xs font-semibold text-slate-600 dark:text-slate-400 mb-1">
                    Exchange
                  </label>
                  <select
                    value={exchange}
                    onChange={(e) => setExchange(e.target.value as StockExchange)}
                    className="w-full px-2.5 py-2 text-sm rounded-xl border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 text-slate-900 dark:text-slate-100"
                  >
                    <option value="NSE">NSE</option>
                    <option value="BSE">BSE</option>
                  </select>
                </div>
              </div>

              <div className="grid grid-cols-2 gap-2.5">
                {side !== 'DIVIDEND' && (
                  <div>
                    <label className="block text-xs font-semibold text-slate-600 dark:text-slate-400 mb-1">
                      Quantity
                    </label>
                    <input
                      type="number"
                      required
                      step="any"
                      min="0.01"
                      value={qtyStr}
                      onChange={(e) => setQtyStr(e.target.value)}
                      placeholder="10"
                      className="w-full px-3 py-2 text-sm font-mono rounded-xl border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 text-slate-900 dark:text-slate-100"
                    />
                  </div>
                )}
                <div className={side === 'DIVIDEND' ? 'col-span-2' : ''}>
                  <label className="block text-xs font-semibold text-slate-600 dark:text-slate-400 mb-1">
                    {side === 'DIVIDEND' ? 'Total Dividend (₹)' : 'Price / Share (₹)'}
                  </label>
                  <input
                    type="number"
                    required
                    step="any"
                    min="0.01"
                    value={priceStr}
                    onChange={(e) => setPriceStr(e.target.value)}
                    placeholder="1325.00"
                    className="w-full px-3 py-2 text-sm font-mono rounded-xl border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 text-slate-900 dark:text-slate-100"
                  />
                </div>
              </div>

              <div className="grid grid-cols-2 gap-2.5">
                <div>
                  <label className="block text-xs font-semibold text-slate-600 dark:text-slate-400 mb-1">
                    Date
                  </label>
                  <input
                    type="date"
                    value={dateIso}
                    onChange={(e) => setDateIso(e.target.value)}
                    className="w-full px-3 py-2 text-sm rounded-xl border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 text-slate-900 dark:text-slate-100"
                  />
                </div>
                <div>
                  <label className="block text-xs font-semibold text-slate-600 dark:text-slate-400 mb-1">
                    Broker / Fees (₹)
                  </label>
                  <div className="flex gap-1.5">
                    <input
                      type="text"
                      value={broker}
                      onChange={(e) => setBroker(e.target.value)}
                      placeholder="Zerodha"
                      className="w-1/2 px-2.5 py-2 text-xs rounded-xl border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 text-slate-900 dark:text-slate-100"
                    />
                    <input
                      type="number"
                      min="0"
                      value={feesStr}
                      onChange={(e) => setFeesStr(e.target.value)}
                      placeholder="25"
                      aria-label="Brokerage and STT fees"
                      className="w-1/2 px-2.5 py-2 text-xs font-mono rounded-xl border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 text-slate-900 dark:text-slate-100"
                    />
                  </div>
                </div>
              </div>

              <div>
                <label className="block text-xs font-semibold text-slate-600 dark:text-slate-400 mb-1">
                  Note (optional)
                </label>
                <input
                  type="text"
                  value={note}
                  onChange={(e) => setNote(e.target.value)}
                  placeholder="e.g. Swing target, long-term SIP"
                  className="w-full px-3 py-2 text-sm rounded-xl border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 text-slate-900 dark:text-slate-100"
                />
              </div>

              <button
                type="submit"
                className="w-full py-3 rounded-2xl text-sm font-semibold text-white snipt-accent-bg snipt-accent-on-solid transition-colors"
              >
                Save {side} Entry
              </button>
            </form>
          </div>
        </div>
      )}
    </main>
  );
}
