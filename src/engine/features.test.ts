import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import {
  DEFAULT_SAVED_CONTACTS,
  looksLikePhoneNumber,
  normalizePhone,
  resolveContactName,
  searchContacts,
  upsertContact,
} from './contacts.ts';
import {
  computeAnalyticsOverview,
  computeGoalPlan,
  type ManualTransaction,
  type SavingsGoal,
} from './personalFinance.ts';
import {
  computeStockPortfolio,
  parseStockSms,
  type StockTrade,
} from './stocks.ts';
import type { FinanceSummary } from './types.ts';

describe('Contacts engine', () => {
  it('normalizes Indian mobile numbers across +91, 0, and spaced formats', () => {
    assert.equal(normalizePhone('+91 98450 12345'), '9845012345');
    assert.equal(normalizePhone('098450-12345'), '9845012345');
    assert.equal(normalizePhone('9845012345'), '9845012345');
  });

  it('distinguishes phone numbers from DLT sender headers', () => {
    assert.equal(looksLikePhoneNumber('+91 98450 12345'), true);
    assert.equal(looksLikePhoneNumber('9876543210'), true);
    assert.equal(looksLikePhoneNumber('VM-HDFCBK-S'), false);
    assert.equal(looksLikePhoneNumber('AX-ZERODH'), false);
  });

  it('respects Contacts permission when resolving sender names', () => {
    assert.equal(
      resolveContactName('+91 98450 12345', DEFAULT_SAVED_CONTACTS, true),
      'Aarav Mehta'
    );
    assert.equal(
      resolveContactName('+91 98450 12345', DEFAULT_SAVED_CONTACTS, false),
      null
    );
  });

  it('upserts and searches saved contacts by name or digits', () => {
    const updated = upsertContact(DEFAULT_SAVED_CONTACTS, {
      name: 'Siddharth Rao',
      phone: '+91 98111 77665',
      label: 'Work',
    });
    assert.equal(resolveContactName('9811177665', updated, true), 'Siddharth Rao');
    const found = searchContacts('77665', updated);
    assert.equal(found.length, 1);
    assert.equal(found[0].name, 'Siddharth Rao');
  });
});

describe('Indian Stock Market parser & FIFO portfolio engine', () => {
  const now = Date.UTC(2026, 9, 25, 10, 0, 0);

  it('parses Zerodha BUY, Groww SELL, and Dividend SMS accurately', () => {
    const buy = parseStockSms(
      'VM-ZERODH-S',
      'Order Executed: BOUGHT 10 shares of INFY on NSE at avg price Rs. 1,875.00 via Zerodha.',
      now
    );
    assert.ok(buy);
    assert.equal(buy.side, 'BUY');
    assert.equal(buy.symbol, 'INFY');
    assert.equal(buy.quantity, 10);
    assert.equal(buy.price, 1875);
    assert.equal(buy.exchange, 'NSE');

    const sell = parseStockSms(
      'AX-GROWW-S',
      'NSE Alert: SOLD 5 shares of HDFCBANK @ Rs. 1,780.00 via Groww.',
      now
    );
    assert.ok(sell);
    assert.equal(sell.side, 'SELL');
    assert.equal(sell.symbol, 'HDFCBANK');
    assert.equal(sell.quantity, 5);
    assert.equal(sell.price, 1780);

    const div = parseStockSms(
      'JD-HDFCBK-S',
      'Dividend of Rs. 680.00 credited to your bank A/C for RELIANCE (20 shares).',
      now
    );
    assert.ok(div);
    assert.equal(div.side, 'DIVIDEND');
    assert.equal(div.symbol, 'RELIANCE');
    assert.equal(div.price, 680);
  });

  it('computes FIFO realized P&L, unrealized P&L, dividends, and STCG/LTCG tax estimate', () => {
    const trades: StockTrade[] = [
      {
        id: 't1',
        symbol: 'TCS',
        exchange: 'NSE',
        side: 'BUY',
        quantity: 10,
        price: 3500,
        fees: 40,
        broker: 'Zerodha',
        timestampMs: Date.UTC(2024, 0, 10), // > 365 days -> LTCG
        source: 'manual',
      },
      {
        id: 't2',
        symbol: 'TCS',
        exchange: 'NSE',
        side: 'BUY',
        quantity: 10,
        price: 3800,
        fees: 40,
        broker: 'Zerodha',
        timestampMs: Date.UTC(2026, 5, 10), // < 365 days -> STCG
        source: 'manual',
      },
      {
        id: 't3',
        symbol: 'TCS',
        exchange: 'NSE',
        side: 'SELL',
        quantity: 12,
        price: 4100,
        fees: 60,
        broker: 'Zerodha',
        timestampMs: Date.UTC(2026, 8, 10),
        source: 'manual',
      },
      {
        id: 't4',
        symbol: 'TCS',
        exchange: 'NSE',
        side: 'DIVIDEND',
        quantity: 8,
        price: 600,
        fees: 0,
        broker: 'Bank Credit',
        timestampMs: Date.UTC(2026, 8, 20),
        source: 'sms',
      },
    ];

    const summary = computeStockPortfolio(trades, { TCS: 4200 });
    assert.equal(summary.holdings.length, 1);
    const tcs = summary.holdings[0];
    assert.equal(tcs.quantity, 8);
    assert.equal(Math.round(tcs.avgBuyPrice), 3804);
    assert.equal(tcs.currentValue, 8 * 4200);
    assert.equal(summary.totalDividends, 600);
    assert.ok(summary.totalRealizedPnl > 0);
    assert.ok(summary.ltcgGains > 0);
    // LTCG is below ₹1.25L exemption and STCG is 0 here because 10 of 12 shares were LTCG (>= 50%)
    assert.equal(summary.estimatedTax, 0);
  });
});

describe('Personal Finance goals & analytics engine', () => {
  it('computes monthly goal contribution and horizon-appropriate Indian strategy', () => {
    const nowMs = Date.UTC(2026, 9, 1);
    const shortGoal: SavingsGoal = {
      id: 'g1',
      title: 'Goa Trip',
      nickname: 'Vacation',
      targetAmount: 100000,
      savedAmount: 20000,
      startDateIso: '2026-10-01',
      targetDateIso: '2027-06-01',
    };
    const shortPlan = computeGoalPlan(shortGoal, nowMs);
    assert.equal(shortPlan.progressPct, 20);
    assert.ok(shortPlan.monthlyContribution > 0);
    assert.match(shortPlan.strategyLabel, /Recurring Deposit|Savings Bank/i);

    const longGoal: SavingsGoal = {
      id: 'g2',
      title: 'Save for retirement',
      nickname: 'Retirement',
      targetAmount: 10000000,
      savedAmount: 0,
      startDateIso: '2026-10-01',
      targetDateIso: '2042-10-01',
    };
    const longPlan = computeGoalPlan(longGoal, nowMs);
    assert.equal(longPlan.breakdown.length, 3);
    assert.ok(longPlan.breakdown.some((b) => b.instrument.includes('NPS')));
    assert.ok(longPlan.breakdown.some((b) => b.instrument.includes('PPF')));
  });

  it('aggregates SMS and manual transactions into Today/Week totals and Monthly chart', () => {
    const nowMs = Date.UTC(2026, 9, 25, 14, 0, 0);
    const financeSummary: FinanceSummary = {
      totalDebits: 450,
      totalCredits: 0,
      netCashflow: -450,
      upiDebitTotal: 450,
      creditCardDebitTotal: 0,
      savingsDebitTotal: 0,
      cardOrBankDebitTotal: 0,
      transactionCount: 1,
      accountBalances: [],
      topMerchants: [],
      transactions: [
        {
          smsId: 1,
          threadId: 10,
          sender: 'VM-HDFCBK-S',
          body: 'Rs. 450 debited via UPI to Swiggy',
          timestampMs: nowMs - 2 * 3600 * 1000,
          simSlot: 0,
          inActiveCycle: true,
          budgetCategory: 'Food & Dining',
          txn: {
            direction: 'Debit',
            amount: 450,
            currency: 'INR',
            paymentMethod: 'UPI',
            accountLast4: '4321',
            merchantOrVpa: 'Swiggy',
            availableBalance: 45000,
            referenceId: null,
            isUpi: true,
          },
        },
      ],
    };
    const manual: ManualTransaction[] = [
      {
        id: 'm1',
        direction: 'Debit',
        amount: 120,
        category: 'Food',
        note: 'Evening tea',
        timestampMs: nowMs - 3600 * 1000,
      },
      {
        id: 'm2',
        direction: 'Credit',
        amount: 50000,
        category: 'Cash',
        note: 'Freelance payout',
        timestampMs: nowMs - 24 * 3600 * 1000,
      },
    ];

    const overview = computeAnalyticsOverview(financeSummary, manual, 'month', nowMs);
    assert.equal(overview.todaySpend, 570);
    assert.equal(overview.weekSpend, 570);
    assert.equal(overview.totalIncome, 50000);
    assert.equal(overview.totalSpend, 570);
    assert.equal(overview.netBalance, 49430);
    assert.equal(overview.series.length, 6);
  });
});
