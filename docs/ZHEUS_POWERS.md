# Zheus' superpowers (Windows and Linux)

*Português: [ZHEUS_POWERS.pt-BR.md](ZHEUS_POWERS.pt-BR.md)*

Zheus — the Hermes Coucou runs, or Mochi's own chat — gets 18 new tools. With them it follows **your portfolio**
against the official data of B3, the CVM, FundosNet and the Banco Central (Brazil's exchange, securities regulator,
fund filings system and central bank), and **your routine**: a public exam to pass, English, training and the month's
content. You just talk to it.

Everything stays on your computer. The official data is public, free and needs no account, and nothing of yours is
sent to those sites. Zheus **never buys, sells, pays, sends or posts anything**: it reads, works things out and keeps
your records.

## Turning it on

1. Settings → **Zheus' superpowers** → turn it on.
2. When Coucou runs Hermes, it gives Hermes the tools itself (`mcp_servers.coucou` in its config.yaml, through Hermes'
   own `hermes config set`) and starts Hermes again once. The green notice "Zheus (Hermes) has the superpowers" says
   when it is ready. For a Hermes you run yourself, the panel shows the lines to add to its config.yaml.
3. A **★** tab appears on the island: the portfolio's value, each class against your target, the passive income and
   what matters today, with questions for Zheus a click away.
4. Give Zheus your portfolio, one of these ways:
   - in the chat: "this is my portfolio:" and paste the table from Investidor10 or your broker;
   - in the panel: paste it and click **Save the portfolio**;
   - import B3's file (Área do Investidor → Extratos): **Posição** (what you hold), **Movimentação** (payouts and
     events) and **Negociação** (trades, for the taxes). XLSX or CSV.

Coucou keeps a copy before it replaces the list (the last 10, in `powers-backups`).

## Trying it: talk to it

1. **The fund guardian** — "How are my real-estate funds?", "What needs me in my funds this month?" (record dates,
   payments, assemblies and their deadlines, offerings, material facts, each with the document's link). If Zheus says
   it does not know a fund's CNPJ (the CVM's reports give some funds no ISIN for their shares, or their old ticker's
   — BTCI11's fund still reports FEXC11's; the island says so too), tell it: "BTCI11's CNPJ is 09.552.812/0001-14"
   (the fund's monthly report on FundosNet shows it). Coucou checks its digits and the name B3 trades the shares
   under, then reads the fund.
2. **Unsustainable dividends** — "Is any of my funds paying more than it earns?" (VP per share falling while it pays,
   negative asset returns, dividends below their average, P/VP; for paper funds, a projection from the Focus survey).
3. **A living thesis** — "My thesis for TIMS3: EBITDA margin above 40%, net debt/EBITDA below 2, revenue growing. Keep
   it and check it." Every new filing is checked against it, and the island says when a condition breaks.
4. **The portfolio's digital twin** — "Where should this month's R$ 500 go?", "How much is missing to reach my targets
   without selling?", "What if the Selic falls 2 points?", "What are my funds' magic numbers?".
5. **The tax auditor** (after importing Movimentação and Negociação) — "Do I have a DARF to pay this month?", "Any
   payout a fund announced that B3 does not show?", "Prepare my 2025 declaration".
6. **An investment committee and a decision journal** — "I want to buy 50 shares of GARE11: run the committee."
   (four voices and a synthesis), "Record it: buy, 70 % sure", and the decision comes back after a month, six months
   and a year.
7. **A public exam, and what an hour of study is worth** — set the exam, its date, the salary, your hours and the
   subjects with their weights; then "What do I study today?", "I studied Economics for 50 min, 7 of 10 right".
8. **English from your portfolio** — "Today's English lesson" (a company of your portfolio, its results to read in
   English, five words), "Turn English mode on", "Quiz me on today's cards".
9. **Training** — "I train 4 times a week, upper/lower, for muscle", "What is today's session?", "Bench 3x10 at 60 kg",
   "I slept 5 hours and I'm wrecked" (the day becomes normal, light or rest).
10. **The month's content kit** — "Make this month's content kit" (square PNG charts in Documents → Coucou → Zheus →
    YYYY-MM, and a carousel, Reels and stories script written with you; in private mode, percentages only).
11. **Your contributions, as you make them** — "Today at 14:30 I bought 10 GGRC11 at R$ 9,20" (or several at once,
    with the costs when you know them): the position's quantity and average price are worked out the way the Receita
    counts them, and the trade is kept for the month's taxes. "I sold 20 GARE11 at 9,10" (it says what you made),
    "Show my contributions this year" (month by month, against the contribution you set), "Undo the last one". A
    spreadsheet of your contributions (Date, Asset, Quantity, Price, and a Type or C/V column if it has sales; .xlsx or
    .csv) imports the same way, and builds the portfolio when it is empty ("rebuild my positions from my trades"
    otherwise).
12. **Ceiling prices, and the opportunities** — "What are my ceiling prices?", "TIMS3's ceiling by Graham", "Use 10 %
    for my funds", "My own ceiling for VALE3 is R$ 70". For funds: Bazin (a year of dividends over the yield you
    want; by default the CDI after the 15 % income tax), projected Bazin (the last three months' pace; a paper fund's
    moved by the Focus survey) and P/VP; for shares: Bazin (6 %), projected Bazin, Graham (√(22,5 × LPA × VPA)) and
    Peter Lynch (LPA × (growth + DY)), from the CVM's statements and share count. When a close falls below a watched
    ceiling (projected Bazin for funds; projected Bazin and Graham for shares; and your own), the island says so by
    itself, once a week at most for each asset unless it falls 5 points further, and it is in the morning's day.

And "What is my day?" — between 7 and 11, once a day, the island says it by itself.

## What it reads, and when

| Source | What | When |
| --- | --- | --- |
| B3 (COTAHIST) | each asset's close | when B3 publishes the day (in the evening) |
| CVM, the funds' monthly reports | VP per share, dividend yield, holders, segment | once a week |
| FundosNet | notices, material facts, assemblies, offerings and dividend notices | once a day |
| CVM, DFP and ITR, and the register (FCA) | the companies' statements and share count | once a month |
| Banco Central (SGS and Focus) | Selic, CDI, IPCA, the dollar and expectations | once a day |

While the superpowers are on, Coucou wakes every half hour to see what is due; when nothing is, nothing is read. A
question to Zheus waits about 20 seconds at most for what is due (and up to 40 for an asset you do not hold); the
reading goes on behind it and the next question has the rest.

## Your data

- `powers.json`, in Coucou's settings folder (`%APPDATA%\Coucou` on Windows, `~/.config/coucou` on Linux): the
  portfolio, the targets, B3's history, the decisions, the theses, the studies, English and training. The copies taken
  before each replacement are in `powers-backups`, beside it.
- `powers-market.json`, in Coucou's local folder (`%LOCALAPPDATA%\Coucou` on Windows, `~/.local/share/coucou` on
  Linux): what the official sources said (it can be deleted; it is read again).
- The content kits: `Documents\Coucou\Zheus\YYYY-MM`.

What the tools return becomes part of the conversation with Zheus' brain, like any message: if the brain is ChatGPT,
that text goes to OpenAI. Your figures go to no one else.

## Safe by default

- Off by default; while it is off, none of these sources is contacted.
- Nothing is bought, sold, paid, sent or posted. The contribution plan is arithmetic on **your** targets, not a
  recommendation of an asset; the committee ends with a question for you, never an order.
- Taxes follow the 2026 rules as Coucou knows them: always check with the Receita's own program before paying or
  declaring.
- Idle with the island hidden, it costs no CPU: the background reading only sleeps until the next half hour.

## What was tested for real

On 21 October 2026 a GitHub Actions workflow (`powers-live.yml`) read the real sources:

- **working**: B3's quotes for six assets; the Selic, CDI, IPCA, dollar and Focus survey from the Banco Central; the
  CVM's reports (VP per share, dividend yield and holders over 21 months) for GGRC11, GARE11, PORD11 and BTCI11;
  FundosNet's documents and dividend notices; the CVM's register of companies and TIM's 2026 quarters; and the whole
  conversation, BTCI11's CNPJ included: asked for, given, checked ("FII BTG CRI" at FundosNet as at B3) and read;
- **fixed since**: dividend notices published twice were counted twice; the yield of a fund with few months of
  notices read came out low (it now goes at the pace of the months known); the CVM published TIM's 2025 DFP with every
  value at zero (it is now left out, and Zheus reads 2026's quarters at a year's pace); the Selic's date showed in the
  future; BTCI11's fund reports the ISIN of its old ticker, FEXC11 (Zheus now asks for the CNPJ); since 2025 the CVM
  writes a fund's receivables as CRI_CRA and LCI_LCA, so paper funds read as brick (now BTCI11, 82 % in CRI, and
  PORD11, 88 %, read as paper); B3's register of listed funds answers nothing any more, so it is no longer asked;
- **a share's ceiling prices**: the share count from the CVM's "composição do capital" was read too (TIM: 2,388,668,322
  shares on 30 June 2026, those in treasury left out), and with it the earnings, book value and dividends a share of
  Bazin, projected Bazin and Graham. TIM's Peter Lynch is left out while the CVM has no earnings for the year before
  (the 2025 DFP came at zero); Zheus can give the growth. FundosNet is slow from abroad; from Brazil it usually answers
  well.

Not tested yet: Hermes calling these tools in a real session on your Windows computer (the tests cover the MCP protocol
and Coucou's side), and the screens on Windows itself (they were looked at in Chromium).
