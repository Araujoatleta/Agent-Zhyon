# Superpoderes do Zheus (Windows e Linux)

*English: [ZHEUS_POWERS.md](ZHEUS_POWERS.md)*

O Zheus — o Hermes que o Coucou roda, ou o chat do próprio Mochi — ganha 18 ferramentas novas. Com elas ele acompanha
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
- Se o Zheus disser que não sabe o CNPJ de um fundo (os informes da CVM trazem alguns fundos sem o ISIN das cotas, ou
  com o do ticker antigo — o fundo do BTCI11 ainda informa o do FEXC11; a ilha também avisa), diga: "O CNPJ do BTCI11 é
  09.552.812/0001-14" (ele aparece no relatório gerencial do fundo na FundosNet). O Coucou confere os dígitos e o nome
  de pregão das cotas na B3 e passa a ler o fundo.

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
- "Quero comprar 50 cotas de GARE11. Faz o comitê." (cinco mentes: valuation, jurídico e governança, contador forense,
  risco e macro, cada uma com o voto, quanta certeza tem, a justificativa e cada ponto marcado como fato, cálculo,
  hipótese, estimativa ou dado ausente, com a fonte; o jurídico e o risco com veto só por critério objetivo; o código
  confere cada número contra o fato ou a página citada; depois dois pareceres separados, A sobre o ativo e B sobre o
  lugar dele na sua carteira contra a sua meta, o placar do presidente, o melhor argumento do lado que perdeu e uma
  pergunta para você). Os fatos que ele recebe trazem fonte, data de referência, data da consulta e a conta, em
  português.
- "O que os relatórios disseram?" (`wealth_documents`: o que o estagiário do iPhone leu, página por página, e as suas
  teses conferidas); "me explica P/VP com os meus fundos", "a minha carteira de treino", "o meu placar"
  (`wealth_learn`).
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

**11. Seus aportes, na hora em que você faz**
- "Hoje às 14h30 comprei 10 GGRC11 a R$ 9,20." (vários de uma vez, com as taxas quando você souber) — ele atualiza a
  quantidade e o preço médio do jeito que a Receita calcula, e guarda a operação para o IR do mês.
- "Vendi 20 GARE11 a 9,10." (ele diz quanto você ganhou ou perdeu) · "Mostra meus aportes deste ano." (mês a mês,
  contra o aporte mensal que você definiu) · "Desfaz o último."
- Uma planilha dos seus aportes (Data, Ativo, Quantidade, Preço, e uma coluna Tipo ou C/V se tiver vendas; .xlsx ou
  .csv) entra do mesmo jeito e monta a carteira quando ela está vazia ("recalcula minhas posições pelos meus aportes"
  nos outros casos).

**12. Preço teto e oportunidades**
- "Quais são os meus preços teto?" · "Qual o preço teto da TIMS3 pelo Graham?" · "Usa 10% para os meus FIIs." ·
  "Meu preço teto para a VALE3 é R$ 70."
- FIIs: Bazin (um ano de dividendos sobre o retorno que você quer; por padrão o CDI depois dos 15% de IR), Bazin
  projetivo (no ritmo dos últimos três meses; o de papel ajustado pelo Focus) e P/VP. Ações: Bazin (6%), Bazin
  projetivo, Graham (√(22,5 × LPA × VPA)) e Peter Lynch (LPA × (crescimento + DY)), com os balanços e o número de ações
  da CVM.
- Quando um fechamento fica abaixo de um teto vigiado (Bazin projetivo nos FIIs; Bazin projetivo e Graham nas ações; e
  o seu), a ilha avisa sozinha — no máximo uma vez por semana por ativo, a não ser que caia mais 5 pontos — e ele
  aparece no seu dia de manhã.

**E o dia todo junto**
- "Como é o meu dia?" Entre 7h e 11h, uma vez por dia, a ilha fala sozinha: "Bom dia! Hoje: …".

## O que ele lê, e quando

| Fonte | O quê | Quando |
| --- | --- | --- |
| Google Finance (a página de cotação) | o preço de cada ativo durante o pregão (uns 15 minutos de atraso) | nos dias de pregão da B3, das 10:00 às 18:30 de Brasília: a cada 5 minutos, a cada minuto com o ★ ou as Configurações abertos, e antes de o Zheus responder sobre a carteira |
| A sua planilha do Google, pelo gog | a aba do AXYON: as suas edições lidas de volta, os números do AXYON e as mudanças do chat escritos | a cada 10 minutos e depois de cada leitura dos preços, no Atualizar, e antes e depois de cada mudança pelo chat |
| B3 (COTAHIST) | o fechamento de cada ativo | quando a B3 publica o dia (à noite) |
| CVM, Informe Mensal dos FIIs | VP/cota, DY, cotistas, segmento | uma vez por semana |
| FundosNet | comunicados, fatos relevantes, assembleias, ofertas e os avisos de rendimentos | uma vez por dia |
| CVM, DFP e ITR, e o cadastro (FCA) | os balanços e o número de ações das empresas | uma vez por mês |
| Banco Central (SGS e Focus) | Selic, CDI, IPCA, dólar e as expectativas | uma vez por dia |

Com os superpoderes ligados, o Coucou acorda a cada meia hora para ver o que venceu; se nada venceu, nada é lido. Uma
pergunta ao Zheus é respondida na hora com o que o AXYON tem, enquanto o que venceu é lido por trás; ela só espera
(cerca de 20 segundos no máximo) quando falta uma cotação ou os números da economia, e até 40 segundos por um ativo que
você não tem. As leituras rodam fora das threads que atendem o AXYON Core, então uma chamada é sempre atendida na hora.

### A sua carteira na sua planilha do Google: a aba do AXYON

Configurações → Superpoderes do Zheus → **Sua planilha do Google**: cole o link da planilha (ou o ID) e clique em
**Salvar e ler**. O AXYON chega à planilha por um de dois caminhos, mostrado embaixo do cartão:

- **O login Google do AXYON Core** — o que a skill google-workspace dele guarda (`google_token.json` na pasta do
  Hermes, criado quando você entrou no Google por lá). O AXYON lê esse arquivo quando precisa, pega com ele um token de
  acesso novo do Google, só na memória (nunca escreve nada de volta), e chama a API do Sheets só para esta planilha. Se
  o AXYON Core ainda não estiver conectado ao Google, peça a ele para configurar a skill google-workspace.
- **O [gog](https://github.com/openclaw/gogcli)**, quando ele estiver no computador, com o login do próprio gog (o
  AXYON nunca vê o login); o campo de conta do gog escolhe uma das contas dele.

Na primeira vez, o AXYON cria a aba dele, **AXYON**, na planilha. Ele lê as suas outras abas uma vez, só lendo: os
tetos delas (uma coluna **Preço teto** ou **Meu teto**) viram os seus no AXYON, os ativos que elas listam são os da
carteira (uma linha com **Status** Vendido ou Zerado fica de fora), e uma posição com quantidade lá substitui a do
AXYON. Depois ele preenche a aba dele, e o que não bate ganha uma nota no **Aviso** da linha: um ativo que o AXYON tinha
e as suas abas não listam ("se vendeu, apague esta linha"), um ativo que as suas abas listam sem quantidade ("preencha
a Quantidade"). A nota some quando você mexe na linha, apaga a linha, ou depois de duas semanas; a ilha avisa que a
aba está pronta e lista as notas, e o AXYON Core fica sabendo também. Daí em diante essa aba é a carteira, e vocês dois
escrevem nela:

| Colunas | Quem escreve |
| --- | --- |
| **Ativo**, **Classe**, **Quantidade**, **Preço médio**, **Meu teto** (A a E) | você, na planilha; e o AXYON, quando você pede no chat (uma compra, uma venda, um teto, um ativo novo ou removido) |
| **Cotação**, **Dia %**, **Valor**, **Custo**, **Resultado**, **Resultado %**, **% da carteira**, **Teto AXYON**, **Método**, **Margem até o teto %**, **Aviso** (F a P) | o AXYON: a cotação agora, a variação do dia, o valor, o custo e o resultado da posição, o peso na carteira, o teto do AXYON e o método, a margem até o seu teto (ou o do AXYON) e o que observar (🟢 abaixo do teto, 🔴 acima, ⚡ dia forte, 📅 data-com, 💰 pagamento hoje) |
| O resumo (R e S) | o AXYON: Patrimônio, Custo, Resultado, Renda passiva / mês, de onde vêm os preços e quando foi escrito |

- Uma linha com ticker da B3 e quantidade é uma posição. Um ticker com **Meu teto** e sem quantidade fica em
  observação: o AXYON cota e avisa quando ele cai abaixo do seu teto. Renda fixa vai pelo nome, com Classe "Renda
  Fixa", o valor aplicado em **Preço médio** e o saldo de agora em **Valor**.
- As suas edições são lidas de volta a cada 10 minutos, depois de cada leitura dos preços no pregão (a cada 5 minutos)
  e no Atualizar. Apagar uma linha tira aquele ativo; esvaziar a aba inteira não zera a carteira (o AXYON preenche de
  novo: para vender tudo, fale com o AXYON).
- Uma mudança pelo chat é escrita na aba na hora, depois de o AXYON ler as suas últimas edições. Se a planilha não
  responder, a mudança é escrita assim que der, antes de a aba ser lida de novo, então nada do que você disse se perde.
- O AXYON escreve só as células que mudaram, como valores (nunca fórmula), e só na aba dele: as suas outras abas nunca
  são escritas. Uma cópia dos seus dados é guardada antes de a planilha mudá-los.
- Para manter o seu próprio layout, ponha o nome da sua aba em **Aba**: o AXYON passa a só ler essa aba (Ativo e
  Quantidade, ou uma lista de tickers com **Preço teto** ou **Meu teto**), e uma mudança pelo chat vale até a próxima
  leitura.
- A cada 3 horas o AXYON também lê, só lendo, duas coisas que as suas abas podem ter: a tabela **Proventos pagos** (os
  meses nas colunas, o total de cada mês, os ativos linha a linha: esses são os seus proventos, no computador e no
  iPhone pareado, com os anúncios dos fundos guardados ao lado para mostrar uma divergência) e a coluna **Percentual
  Ideal** (ou **Meta**) ao lado dos ativos (a sua meta por ativo, que o comitê leva em conta). A meta também pode ser
  dita no chat (`wealth_update` com `asset_targets`, como `{"GARE11": 7.5}`).

### O preço durante o pregão

Durante o pregão da B3, a carteira, os preços-teto e as oportunidades usam o preço do Google Finance (a página de
cotação: sem chave, sem conta), mostrado no ★ como "Google Finance às 15:16". O Google entrega as cotações da B3 com
uns 15 minutos de atraso. À noite, o fechamento oficial da B3 volta a valer.

### O que o AXYON avisa na hora

A cada leitura dos preços do pregão, o AXYON avisa na ilha, no momento em que acontece:

- 🟢 um ativo que cai abaixo do **seu** teto (estava acima no último fechamento), ou que está abaixo de um dos tetos
  dele pela sua margem (uma oportunidade, avisada uma vez por semana);
- 🔴 um ativo que volta para cima do seu teto;
- 📈 📉 um dia forte de um ativo que você tem ou observa, 3 % e depois 6 % para cima ou para baixo, com quanto a sua
  posição fez;
- 📈 📉 um dia forte da carteira inteira, 2 % e depois 4 %.

Cada aviso vem uma vez por dia. A ilha abre no ★ com o aviso no lugar das perguntas (**Perguntar ao AXYON** escreve uma
pergunta sobre ele no chat, para você enviar), o AXYON faz uma cara e um som, o aviso entra no chat e é lido em voz
alta se você ligou isso. A coluna "Aviso" da aba do AXYON diz o mesmo, o tempo todo.

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
  informes da CVM (VP/cota, DY e cotistas de 21 meses) do GGRC11, do GARE11, do PORD11 e do BTCI11, os documentos e os
  avisos de rendimentos da FundosNet, o cadastro de empresas da CVM e os trimestres de 2026 da TIM; e a conversa
  inteira, com o CNPJ do BTCI11: pedido, informado, conferido ("FII BTG CRI" na FundosNet e na B3) e lido;
- **corrigido depois disso**: avisos de rendimentos publicados duas vezes contavam em dobro; o DY de quem ainda tinha
  poucos meses de avisos lidos saía baixo (agora vai no ritmo dos meses conhecidos); a CVM publicou a DFP 2025 da TIM
  com todos os valores zerados (agora é ignorada, e o Zheus usa os trimestres de 2026 no ritmo do ano); a data da
  Selic aparecia no futuro; o fundo do BTCI11 informa o ISIN do ticker antigo, FEXC11 (agora o Zheus pede o CNPJ);
  desde 2025 a CVM registra os recebíveis dos fundos em CRI_CRA e LCI_LCA, e os fundos de papel saíam como tijolo
  (agora o BTCI11, 82% em CRI, e o PORD11, 88%, saem como papel); o cadastro de fundos listados da B3 não responde mais
  nada, então deixou de ser consultado;
- **o preço teto das ações**: o número de ações da "composição do capital" da CVM também foi lido (TIM: 2.388.668.322
  ações em 30/06/2026, já sem as em tesouraria), e com ele o LPA, o VPA e o dividendo por ação do Bazin, do Bazin
  projetivo e do Graham. O Peter Lynch da TIM fica de fora enquanto a CVM não tem o lucro do ano anterior (a DFP 2025
  veio zerada); o Zheus pode passar o crescimento. A FundosNet é lenta vista do exterior; do Brasil costuma responder
  bem.

Ainda não testado: o Hermes chamando estas ferramentas numa sessão real no seu Windows (os testes cobrem o protocolo MCP
e o Coucou), e as telas no Windows (vistas no Chromium daqui).
