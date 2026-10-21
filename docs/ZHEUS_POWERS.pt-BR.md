# Superpoderes do Zheus (Windows e Linux)

*English: [ZHEUS_POWERS.md](ZHEUS_POWERS.md)*

O Zheus — o Hermes que o Coucou roda, ou o chat do próprio Mochi — ganha 16 ferramentas novas. Com elas ele acompanha
**a sua carteira** com os dados oficiais da B3, da CVM, da FundosNet e do Banco Central, e **a sua rotina**: o
concurso, o inglês, a academia e o conteúdo do mês. É só conversar com ele.

Tudo fica no seu computador. Os dados oficiais são públicos, gratuitos e sem conta, e nada seu é enviado a esses sites.
O Zheus **nunca compra, vende, paga, envia ou publica nada**: ele lê, calcula e guarda os seus registros.

## Ligando

1. Configurações → **Superpoderes do Zheus** → ligue a chave.
2. Se é o Coucou que roda o Hermes, ele mesmo entrega as ferramentas ao Hermes (`mcp_servers.coucou` no config.yaml,
   pelo próprio `hermes config set`) e reinicia o Hermes uma vez. O aviso verde "O Zheus (Hermes) já tem os
   superpoderes" diz quando está pronto. Se você roda um Hermes por conta própria, o painel mostra as linhas que entram
   no config.yaml dele.
3. Aparece uma aba **★** na ilha: o valor da carteira, cada classe contra a sua meta, a renda passiva e o que importa
   hoje, com perguntas prontas para o Zheus.
4. Passe a sua carteira ao Zheus, de um destes jeitos:
   - no chat: "essa é a minha carteira:" e cole a tabela do Investidor10 ou da corretora;
   - no painel: cole a tabela e clique em **Salvar a carteira**;
   - importe o arquivo da B3 (Área do Investidor → Extratos): **Posição** (o que você tem), **Movimentação**
     (proventos e eventos) e **Negociação** (operações, para o IR). XLSX ou CSV.

O Coucou guarda uma cópia antes de substituir a lista (as 10 últimas, em `powers-backups`).

## Para testar: converse com ele

Os exemplos abaixo são frases que você pode mandar como estão, na ilha, no iPhone ou no chat do Hermes.

**1. Guardião dos FIIs**: dividendos, VP/cota e tudo o que pede algo de você.
- "Como estão meus FIIs?"
- "O que precisa de mim nos meus fundos este mês?" (data-com, pagamentos, assembleias com prazo, ofertas, fatos
  relevantes, com o link do documento)

**2. Detector de dividendo insustentável**
- "Algum FII meu está pagando mais do que ganha?" (VP/cota caindo enquanto paga, retorno dos ativos negativo,
  dividendo abaixo da média, P/VP; para fundos de papel, a projeção pelo Focus)

**3. Tese viva**: os balanços da empresa na CVM contra a sua tese.
- "Minha tese da TIMS3: margem EBITDA acima de 40%, dívida líquida/EBITDA abaixo de 2, receita crescendo. Guarda e
  confere."
- "A tese da TIMS3 ainda vale?" (a ilha avisa quando uma condição quebra)

**4. Gêmeo digital da carteira**
- "Onde aporto R$ 500 este mês?" (por classe, contra as suas metas, sem vender)
- "Quanto falta para chegar nas minhas metas sem vender nada?"
- "E se a Selic cair 2 pontos e o IPCA subir 1?"
- "Qual o número mágico dos meus FIIs?"

**5. Auditor e IR** (depois de importar Movimentação e Negociação)
- "Tenho DARF para pagar este mês?" (isenção de R$ 20 mil em ações, FIIs a 20%, ETFs, day trade, prejuízo a
  compensar, código 6015 e vencimento)
- "Algum provento que o fundo anunciou e a B3 não mostra?"
- "Monta a minha declaração de 2025" (bens e direitos com os códigos, isentos, JCP)

**6. Comitê de investimento e diário de decisões**
- "Quero comprar 50 cotas de GARE11. Faz o comitê." (quatro vozes: a favor, o cético, o risco, IR e custos; e a síntese)
- "Registra: comprar 50 GARE11, porque o P/VP está abaixo de 1, confiança 70%."
- Um mês, seis meses e um ano depois a decisão volta: "Revisa minhas decisões."

**7. Concurso e o valor de uma hora de estudo**
- "Vou prestar Analista do BACEN, Cebraspe, prova em 18/04/2027, salário 20.924,80, estudo 15 h por semana. Matérias:
  Economia peso 30 nível 2, Português peso 15 nível 4, Finanças peso 20 nível 1."
- "Qual o meu plano de estudo de hoje?" / "Estudei 50 min de Economia, 10 questões, 7 certas."
- "Quanto vale uma hora de estudo para mim?"

**8. Inglês com a sua carteira**
- "Me dá a aula de inglês de hoje." (uma empresa da carteira, os resultados para ler em inglês, cinco palavras)
- "Liga o modo inglês." (ele responde em inglês e corrige as suas frases)
- "Me pergunta os cartões de hoje." (repetição espaçada, também para o concurso)

**9. Academia**
- "Treino 4 vezes por semana, superior/inferior, hipertrofia."
- "Qual o treino de hoje?" (cada exercício com a carga a tentar, por dupla progressão)
- "Fiz supino 3x10 com 60 kg e remada 3x10 com 50." / "Dormi 5 horas e estou moído." (ele ajusta o dia: normal, leve
  ou descanso)

**10. Kit de conteúdo do mês**
- "Monta o kit de conteúdo do mês." (gráficos quadrados em PNG em Documentos → Coucou → Zheus → AAAA-MM, e um roteiro
  de carrossel, Reels e stories com você; no modo privado, só porcentagens)

**E o dia todo junto**
- "Como é o meu dia?" Entre 7h e 11h, uma vez por dia, a ilha fala sozinha: "Bom dia! Hoje: …".

## O que ele lê, e quando

| Fonte | O quê | Quando |
| --- | --- | --- |
| B3 (COTAHIST) | o fechamento de cada ativo | quando a B3 publica o dia (à noite) |
| CVM, Informe Mensal dos FIIs | VP/cota, DY, cotistas, segmento | uma vez por semana |
| FundosNet | comunicados, fatos relevantes, assembleias, ofertas e os avisos de rendimentos | uma vez por dia |
| CVM, DFP e ITR, e o cadastro (FCA) | os balanços das empresas | uma vez por mês |
| Banco Central (SGS e Focus) | Selic, CDI, IPCA, dólar e as expectativas | uma vez por dia |
| B3, cadastro de fundos listados | o CNPJ de um fundo que a CVM publica sem ISIN | só quando falta |

Com os superpoderes ligados, o Coucou acorda a cada meia hora para ver o que venceu; se nada venceu, nada é lido. Uma
pergunta ao Zheus espera no máximo cerca de 20 segundos pelo que estiver vencendo (e até 40 segundos por um ativo que
você não tem); a leitura continua por trás e a próxima pergunta já tem o resto.

## Os seus dados

- `powers.json`, na pasta de configurações do Coucou (`%APPDATA%\Coucou` no Windows, `~/.config/coucou` no Linux): a
  carteira, as metas, o histórico da B3, as decisões, as teses, os estudos, o inglês e os treinos. As cópias antes de
  cada substituição ficam em `powers-backups`, ao lado.
- `powers-market.json`, na pasta local do Coucou (`%LOCALAPPDATA%\Coucou` no Windows, `~/.local/share/coucou` no
  Linux): o que as fontes oficiais disseram (pode ser apagado; é lido de novo).
- Os kits de conteúdo: `Documentos\Coucou\Zheus\AAAA-MM`.

O que as ferramentas devolvem vira parte da conversa com o cérebro do Zheus, como qualquer mensagem: se o cérebro é o
ChatGPT, esse texto vai para a OpenAI. Os seus números não são enviados a mais ninguém.

## Seguro por padrão

- Desligado por padrão; sem os superpoderes ligados, nenhuma dessas fontes é contactada.
- Nada é comprado, vendido, pago, enviado ou publicado. O plano de aportes é conta sobre as **suas** metas, não uma
  recomendação de ativo; o comitê termina com uma pergunta para você, nunca com uma ordem.
- Os impostos seguem as regras de 2026 como o Coucou as conhece: confira sempre no programa da Receita antes de pagar
  ou declarar.
- Parado e com a ilha escondida, não gasta CPU: a leitura em segundo plano só dorme até a próxima meia hora.

## O que já foi testado de verdade

Em 21/10/2026, um workflow do GitHub Actions (`powers-live.yml`) leu as fontes reais:

- **funcionando**: as cotações da B3 dos seis ativos, Selic, CDI, IPCA, dólar e o Focus do Banco Central, os
  informes da CVM (VP/cota, DY e cotistas de 21 meses) do GGRC11, do GARE11 e do PORD11, os documentos e os avisos de
  rendimentos da FundosNet, o cadastro de empresas da CVM e os trimestres de 2026 da TIM;
- **corrigido depois disso**: avisos de rendimentos publicados duas vezes contavam em dobro; o DY de quem ainda tinha
  poucos meses de avisos lidos saía baixo (agora vai no ritmo dos meses conhecidos); a CVM publicou a DFP 2025 da TIM
  com todos os valores zerados (agora é ignorada, e o Zheus usa os trimestres de 2026 no ritmo do ano); a data da
  Selic aparecia no futuro;
- **em verificação**: o BTCI11 vem sem ISIN nos informes da CVM, então o Coucou passa a buscar o CNPJ no cadastro de
  fundos da B3. A FundosNet é lenta vista do exterior; do Brasil costuma responder bem.

Ainda não testado: o Hermes chamando estas ferramentas numa sessão real no seu Windows (os testes cobrem o protocolo MCP
e o Coucou), e as telas no Windows (vistas no Chromium daqui).
