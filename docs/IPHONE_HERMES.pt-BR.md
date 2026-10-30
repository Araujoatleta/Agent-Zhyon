# AXYON no iPhone com o Hermes dentro

O app do iPhone instalado por sideload (Sideloadly ou AltStore) pode levar o **Hermes Agent dentro dele**. O Hermes é o
cérebro e o próprio iPhone é o corpo: o AXYON pensa e age **sem computador nenhum**. Um computador Windows ou Linux
continua sendo uma opção: pareado, ele acrescenta a casa dele, os agentes (Claude Code, Codex…) e a carteira.

## Instalar

1. No GitHub, aba **Actions** → **iPhone (sideload)** → a última execução verde → baixe o artefato
   **AXYON-iPhone-Hermes** (é um .zip com o `AXYON-iPhone-Hermes.ipa` dentro).
2. No computador, abra o **Sideloadly** (ou AltStore), arraste o .ipa, entre com o seu Apple ID e instale, como o app
   anterior. O ícone aparece como **AXYON**.
3. No iPhone: **Ajustes → Geral → VPN e Gerenciamento de Dispositivo** → confie no seu Apple ID. Com Apple ID grátis,
   o app precisa ser reinstalado a cada 7 dias (o Sideloadly faz isso de novo com o mesmo arquivo).
4. No iOS 16 ou mais novo, na primeira vez: **Ajustes → Privacidade e Segurança → Modo de Desenvolvedor** → ligue (o
   iPhone reinicia e pergunta de novo: confirme).

## Primeiro uso

1. Abra o AXYON. O orbe acorda: o Hermes leva alguns segundos para iniciar dentro do iPhone na primeira vez.
2. Toque em **Dê um cérebro ao Hermes** (ou **Ajustes → Cérebro**) e escolha:
   - **ChatGPT** (a sua assinatura, sem chave): toque em **Entrar com o ChatGPT**, copie o código, toque em **Abrir a
     OpenAI**, cole o código e confirme. Esse login é só deste iPhone, separado do computador.
   - **Google Gemini** (chave grátis): **Pegar uma chave grátis no Google AI Studio**, copie a chave, cole e toque em
     **Usar este cérebro**.
   - **OpenRouter**, **Anthropic** ou **outro compatível com OpenAI** (o seu servidor): cole a chave (e o endereço).
   As chaves ficam nas Chaves (Keychain) deste iPhone e não vão para backup.
3. Pronto: escreva ou fale no **Chat**.

## O que ele faz pelo iPhone

Peça normalmente, por texto ou voz (o microfone do chat; uma pausa envia):

- **Lembretes e notificações**: *"me lembra às 18h de pagar a conta"*, *"o que tenho de lembrete?"*, *"marca como
  feito"*. A notificação chega mesmo com o app fechado.
- **Calendário**: *"o que eu tenho hoje e amanhã?"*, *"marca reunião sexta às 10h com o João"*.
- **Contatos e mensagens**: *"qual o telefone da Ana?"*, *"manda um WhatsApp para a Ana dizendo que chego às 8"*: ele
  abre o WhatsApp (ou Mensagens, Mail, Telefone) com tudo preenchido, e **você** envia. Ele nunca envia nem liga
  sozinho.
- **Atalhos**: *"roda o meu atalho Boa noite"*. Os Atalhos do iPhone fazem quase tudo (alarmes, Foco, Wi-Fi, cenas
  da Casa da Apple): crie o atalho uma vez e peça ao AXYON pelo nome.
- **Apps e links**: abrir o Maps com um endereço, o YouTube, o Spotify, uma página.
- **O iPhone**: bateria, armazenamento, onde você está, lanterna, música (tocar, pausar, próxima), ler em voz alta, a
  área de transferência.
- **Web**: *"pesquisa o horário da farmácia aqui perto"*, *"lê esta página e me resume"*. Sem chave nem conta.
- **Memória**: o Hermes lembra do que você contar e das conversas anteriores, como no computador.
- **Ajudantes**: num trabalho longo, o Hermes divide partes com ajudantes dele (delegação), como no computador.

## Fotos e arquivos

O **+** ao lado do campo de mensagem manda para o Hermes **fotos** (da galeria ou da **câmera**) e **arquivos** (do
app Arquivos ou do iCloud Drive): PDF, Word, Excel, CSV, texto. Um cérebro que enxerga (ChatGPT, Gemini, Claude)
recebe a própria foto; outro recebe a foto descrita antes. O PDF é lido no próprio iPhone (as páginas escaneadas,
com o reconhecimento de texto do iPhone) e o texto vai junto com a sua mensagem; o Hermes consegue ler de novo
qualquer arquivo que você mandou. *"O que diz esta nota?"*, *"resume este extrato"*, *"lança estas operações na minha
carteira"*. Os arquivos ficam na pasta do próprio AXYON, por um mês no máximo.

Com o ChatGPT como cérebro (ou o OpenRouter), o Hermes também **cria imagens** (*"desenha…"*): elas aparecem embaixo da
resposta, e um toque compartilha ou salva.

## Google: Gmail, Planilhas, Drive, Documentos e Agenda

No computador, a skill google-workspace do Hermes faz o login rodando scripts num terminal, e nenhum app de iPhone
pode fazer isso. Aqui o próprio AXYON faz o login no Google, dentro do app, com **o seu cliente OAuth do Google
Cloud** (o mesmo que você fez para o gog ou para o Hermes no computador serve).

1. Uma vez, no Google Cloud (uns 5 minutos): crie um projeto, ative as APIs do Gmail, Sheets, Drive, Docs e Calendar,
   crie um cliente OAuth do tipo **App para computador (Desktop app)**, adicione a sua conta como usuário de teste
   (Público-alvo) e baixe o JSON.
2. **Ajustes → Google → Escolher o arquivo JSON do cliente** (ou cole o ID e a chave secreta do cliente), ou mande o
   JSON no chat e peça *"conecta o meu Google"*.
3. **Entrar com o Google**: a própria página do Google abre dentro do AXYON; entre e permita (se o Google disser que o
   app não foi verificado: Avançado → ir para o seu app). O AXYON termina sozinho. Se a página depois de permitir não
   carregar, copie o endereço dela e cole em Ajustes → Google, ou no chat.

Depois é só pedir: *"o que tem de novo no meu Gmail?"*, *"procura no e-mail o relatório do BTLG11"*, *"lê a aba AXYON
da minha planilha"*, *"lança esta operação na minha planilha"*, *"acha o meu plano 2027 no Drive"*, *"coloca a
assembleia na minha agenda do Google"*. Um rascunho vai para os rascunhos do Gmail; **um e-mail só é enviado quando você
toca em Enviar** no cartão que o AXYON mostra no chat. A sua senha nunca passa pelo AXYON: o login é a página do
Google, e o AXYON guarda o acesso do Google na pasta do Hermes, como o Hermes faz no computador.

## Tarefas agendadas

O **cron** do próprio Hermes funciona no iPhone: *"todo dia útil às 9h, me diga como estão o BTLG11 e o LVBI11 em
relação aos meus tetos"*, *"daqui a 2 horas, me lembra de pagar o DARF com o valor"*. A resposta de cada tarefa chega
como **notificação**; uma tarefa que deve ficar quieta quando não há nada a dizer ("só me avisa se…") fica quieta.
**Ajustes → Tarefas agendadas** mostra as tarefas; para mudar ou tirar uma, peça no chat.

Na hora certa com o AXYON aberto. Com o AXYON fechado, o iOS acorda o app **algumas vezes por dia** (com a
Atualização em 2º Plano ligada), e a tarefa que venceu roda nessa hora: nenhum app de iPhone pode se acordar a cada 5
minutos. Para alertas em minutos durante o pregão, o AXYON do computador acompanha os preços enquanto está aberto e
pode avisar o iPhone (no computador: Ajustes → Celular → Notificações com o Coucou fechado).

## Os superpoderes, dentro do iPhone

O mesmo motor dos superpoderes do computador roda dentro do app, sem computador:

- **Carteira**: *"como está a minha carteira hoje?"*, *"quanto está o GARE11?"*. Com os dados oficiais da B3, da CVM,
  do FundosNet e do Banco Central (lidos na hora que você pergunta), e os preços do pregão pelo Google Finance (com
  uns 15 minutos de atraso).
- **Registrar o que você fez**: *"hoje comprei 10 GGRC11 a 9,20"*, *"cola a minha carteira"* (texto do Investidor10 ou
  da corretora), *"o meu teto para TIMS3 é 21,50"*. Fica guardado neste iPhone e aparece na aba Carteira.
- **FIIs e empresas**: valor patrimonial, P/VP, dividendos e se são sustentáveis, data-com e pagamentos, assembleias;
  os balanços de uma empresa e a sua tese conferida item a item.
- **Preço teto** (Bazin, Bazin projetivo, Graham, Peter Lynch) e as oportunidades abaixo do teto.
- **Plano, impostos e decisões**: o caminho até as suas metas por classe, o DARF do mês e a declaração anual, o
  diário de decisões e o *comitê* antes de decidir.
- **Estudos, inglês e treino**: o plano do concurso, os cartões de revisão, a aula de inglês com as empresas da sua
  carteira, o treino do dia e *"como é o meu dia hoje?"*.

Nada disso compra, vende, paga ou envia: o AXYON só registra e calcula. O kit de conteúdo do mês continua no
computador.

**Alertas com o AXYON fechado** (Ajustes → Superpoderes, desligados até você ligar): uma notificação quando um ativo
cai abaixo do seu preço teto ou volta para cima dele, num dia forte de um ativo ou da carteira, e quando um ativo está
abaixo do teto, cada coisa uma vez; e de manhã, uma vez, **o seu dia** (data-com e pagamentos, DARF, decisões para
rever, estudos, treino e a aula de inglês). Quem decide quando o AXYON pode olhar é o iOS (algumas vezes por dia, conforme você
usa o iPhone, e só com **Ajustes → Geral → Atualização em 2º Plano** ligada para o AXYON); **Verificar agora** olha
na hora. Só a B3, a CVM, o FundosNet, o Banco Central e o Google Finance são lidos; nada sobre você é enviado.

Cada coisa que o iPhone protege (calendário, lembretes, contatos, localização, notificações) o iOS pergunta na
primeira vez. **Ajustes → O que o AXYON pode usar** mostra o que está liberado.

## Investimentos: o comitê, o estagiário, as suas teses, a semana e o aprender

A aba **Carteira**, a primeira, onde o AXYON abre, é a tela de investimentos, no design de uma tela de estatísticas:
grafite, um anel de segmentos grossos e arredondados em cores suaves com o total no meio, a legenda em duas colunas,
uma pílula para escolher **Carteira** ou **Proventos**, **Classe | Ativo**, um carrossel de meses para os proventos e
**Histórico** embaixo (as suas posições, como antes). Todo número é calculado pelo código do AXYON a partir das
fontes oficiais; o cérebro só lê e vota, e diz de onde vem cada ponto. Os relatórios saem sempre em português.

- **Os proventos são os da sua planilha**: quando a sua planilha do Google tem a tabela **Proventos pagos** (os meses
  nas colunas, o total de cada mês, os ativos linha a linha), o computador lê a cada 3 horas e manda junto com a cópia
  da carteira; o anel mostra esses valores (R$ 82,27 em outubro, se é o que a planilha diz), indica **Sua planilha ·
  consultado em …**, e um mês em que os anúncios dos fundos discordam mostra os dois. Sem a tabela, os anúncios vezes
  o que você tinha.
- **Metas por ativo**: a coluna **Percentual Ideal** (ou **Meta**) da sua planilha, ou **… → Metas por ativo** na
  Carteira. O comitê pesa cada ativo contra a meta antes de qualquer parecer sobre comprar mais.
- **O comitê de cinco mentes, auditado**: pergunte *"comprar 100 cotas de GGRC11?"*. Antes, o código junta os
  **fatos**, numerados (F1…), cada um com a fonte, a data de referência, o dia da consulta, a conta (o P/VP diz o dia
  do preço e o mês do VP; R$ 0,083 continua R$ 0,083 até o DY; o DY de 12 meses e o anualizado pelo último rendimento
  são dois fatos) e um status: confirmado, calculado, estimativa, ausente ou divergente (duas fontes que discordam).
  Um analista de valuation, o jurídico e governança, um contador forense, um gestor de risco e um estrategista macro
  votam, cada um com quanta certeza tem, a justificativa e pontos marcados como fato, cálculo, hipótese, estimativa ou
  dado ausente, citando F3 ou a página de um documento (D1 p.4), e o que faltou. Depois, o código confere cada número
  contra o que foi citado, transforma em neutro um voto contra que só se apoia em dado ausente, mantém um veto só com
  critério objetivo (concentração só acima do dobro da sua meta ou de um quarto da carteira), e um **auditor** (o
  cérebro de novo) revisa tudo, corrige as mentes que precisar e escreve os dois pareceres finais: **A, o ativo**
  (positivo, neutro, negativo) e **B, na sua carteira** (aumentar, manter, reduzir, evitar novo aporte) — um ativo
  bom acima da meta pode ter A positivo e B evitar novo aporte, e votar contra uma compra nunca é recomendar venda. O
  veredito mostra os **Alertas críticos** separados dos **Pontos materiais** e das **Observações**, a lista **A
  verificar**, os pontos de cada mente com um selo quando o código os achou na fonte, e as **Fontes**: toque em F3
  para ver a fonte, as datas e a conta, ou em D1 p.4 para o texto da página e o link do documento. Termina com uma
  **pergunta**: a decisão é sua. As mentes são arquétipos, nunca pessoas reais.
- **O estagiário que lê tudo**: os relatórios gerenciais, fatos relevantes, assembleias e ofertas dos seus fundos
  (FundosNet) e os fatos relevantes, comunicados e releases das suas empresas (a lista da CVM, uma vez por semana) são
  baixados e lidos no iPhone (PDFKit, as páginas escaneadas pelo reconhecimento de texto do próprio iPhone); o código
  acha os números com as páginas (vacância, inadimplência, rendimento e resultado por cota, LTV, cap rate, cotistas) e
  o que mudou desde o relatório anterior; o AXYON Core resume só o que importa, página por página.
- **Tese viva**: escreva com as suas palavras por que você tem um ativo (*"P/VP abaixo de 1,05; DY acima de 9%;
  vacância até 10%"*); cada condição com número é conferida no preço e a cada relatório novo. Quando sai um documento
  que importa ou uma tese quebra, o comitê **se reúne sozinho** quando o AXYON Core está rodando, e você recebe uma
  notificação (com os alertas de **Ajustes → Superpoderes** ligados).
- **Sua semana em 3 minutos**: um roteiro feito pelo código do AXYON com os seus registros (a carteira contra a
  semana passada, as maiores altas e quedas, os proventos que caíram e os que vêm, o que o estagiário leu, as teses,
  o comitê), lido em voz alta pela voz do iPhone; **Gerar o áudio** salva um arquivo de som para enviar (o WhatsApp
  recebe como áudio), **WhatsApp** abre com o texto escrito, e um lembrete todo domingo se você quiser.
- **Aprenda com a sua carteira**: modo aprendiz (o comitê, o estagiário e a semana explicam cada termo na primeira
  vez), um glossário com o valor de cada termo nos seus próprios ativos, flashcards dos seus ativos, uma carteira de
  treino com dinheiro de mentira pelos preços reais (contra a sua real e o CDI) e o seu placar pessoal (quando você
  diz que tem certeza, quantas vezes acerta, pelo diário de decisões).

A sua carteira fica no iPhone. Os documentos são públicos e lidos uma vez no próprio iPhone; o cérebro que você
escolheu só é chamado para resumir um documento e para as reuniões do comitê (duas respostas: as mentes e o auditor).
Nada é comprado nem vendido: isto não é recomendação de investimento.

## A casa direto pelo iPhone (sem computador)

Na aba **Casa → +** você adiciona os aparelhos que o próprio iPhone controla pelo Wi-Fi de casa:

- **TV Samsung** (Tizen, desde 2016): **Procurar neste Wi-Fi** ou digite o endereço da TV. Na primeira vez a TV pergunta
  na tela se permite o **AXYON**: aceite. Depois: ligar/desligar, volume, canais, abrir apps, setas do controle.
- **Android TV ou TV box** (inclusive Android 4.4): no aparelho, **Configurações → Sobre → toque 7 vezes em
  Versão/Build**, depois **Opções do desenvolvedor → Depuração USB** e **Depuração de rede/ADB pela rede (porta 5555)**.
  Adicione pelo endereço; o aparelho pergunta **“Permitir depuração USB?”**: marque **Sempre permitir** e aceite. A
  chave fica nas Chaves do iPhone.
- **Home Assistant**: o endereço (ex.: `http://homeassistant.local:8123`) e um **token de longa duração** (no Home
  Assistant: seu perfil → Segurança → Tokens de acesso de longa duração → Criar token). Luzes, tomadas, players,
  cortinas, cenas e scripts aparecem sozinhos. Fechaduras e alarmes nunca.

Depois é só pedir no chat: *"liga a TV da sala"*, *"abre o YouTube na TV do quarto"*, *"apaga a luz da sala"*,
*"aumenta 5 de volume"*. Tomadas e interruptores pedem o seu **Face ID** antes. Na primeira busca o iOS pergunta se
o AXYON pode usar a **rede local**: permita (Ajustes → Privacidade e Segurança → Rede Local → AXYON).

Ligar uma TV que está totalmente desligada pela rede nem sempre dá: o iOS não deixa apps mandarem o sinal de
"acordar" para a rede toda. Deixe ligada nas TVs Samsung a opção **Ligar com o celular** (Configurações → Geral →
Rede → Configurações avançadas).

## As telas

- **Carteira** (a primeira, onde o app abre): os investimentos (o anel da carteira e dos proventos, o comitê, o
  estagiário, as teses, a semana e o aprender); em **Histórico**, o total, o dia, cada posição com o seu preço teto e
  os ativos que você acompanha.
- **Chat**: a conversa com o Hermes, com cada passo que ele dá (lendo a carteira, agendando, pesquisando…), parar,
  ditado e leitura em voz alta.
- **Casa**: os aparelhos que o iPhone controla sozinho e, com um computador pareado, a casa dele.
- **Ajustes**: o cérebro, as permissões, os alertas dos superpoderes, os agentes do computador, o computador
  (opcional) e o resto dos ajustes do app.

Um comando de um agente do computador esperando o seu OK aparece acima de qualquer aba, com Permitir (Face ID) e
Negar. O app fala português (definido uma vez; **Ajustes do iOS → AXYON → Idioma** muda).

## Com um computador (opcional)

**Ajustes → Computador (opcional) → Computador Windows ou Linux** pareia como antes (código ou QR). Com ele pareado:

- a **carteira** do AXYON do computador (posições, os seus tetos, os últimos preços e os avisos da aba do AXYON) passa
  para o iPhone sempre que ele se conecta, e fica guardada: com o computador desligado, o Hermes do iPhone continua
  respondendo sobre ela, com os preços de agora. Cada cópia nova do computador substitui as posições do iPhone: com
  um computador, ele e a sua planilha continuam sendo o registro;
- a **casa** do computador aparece na aba Casa e o Hermes pode ligar e desligar os aparelhos dela;
- os **agentes** do computador aparecem em **Ajustes → Agentes no computador**, e um comando esperando o seu OK
  aparece acima de qualquer aba, com Permitir/Negar;
- o **chat com o AXYON do computador** e os ajustes do assistente de lá continuam em Ajustes.

Fora de casa, use o Tailscale nos dois ([IPHONE_PC.pt-BR.md](IPHONE_PC.pt-BR.md)).

## Limites do iOS

- O iOS suspende os apps pouco depois de saírem da tela: o Hermes pensa com o AXYON aberto. Uma resposta em andamento
  ganha o meio minuto que o iOS dá para terminar, e uma notificação traz a resposta.
- O iOS não deixa um app rodar outros programas: o terminal do Hermes, a execução de código, o navegador dele e as
  ferramentas de arquivo que passam por um shell não existem no iPhone (no computador, sim).
- **Uso do computador** (o mouse e o teclado que o Hermes usa pelo cua-driver no Mac ou no PC) não tem como existir
  no iPhone: o iOS não deixa nenhum app ver ou tocar na tela de outro app, com sideload ou não. O que mais chega perto:
  os **Atalhos** (*"roda o meu atalho…"*: os Atalhos alcançam quase todo ajuste e muitos apps), os links que abrem um
  app já preenchido, e o AXYON do computador pareado, que usa a tela dele.
- Se o Hermes não iniciar, o início mostra o motivo; feche o AXYON no seletor de apps e abra de novo.

## Se o app fechar sozinho ou o Hermes não iniciar: o REA

O [REA](https://github.com/morluto/rea) lê o IPA e os relatórios de falha do iPhone sem executar nada. Num computador
com o [Node.js](https://nodejs.org) 22 ou 24:

1. **O que o IPA carrega** (Info.plist, frameworks, o Python, as permissões):
   `npx -y rea-agents@latest inspect-artifact AXYON-iPhone-Hermes.ipa`
2. **Um relatório de falha do iPhone**: no iPhone, **Ajustes → Privacidade e Segurança → Análise e Melhorias → Dados
   de Análise**, procure um arquivo que começa com **Coucou** (o nome do programa do AXYON), toque nele e compartilhe
   (AirDrop ou e-mail para você mesmo). No computador:
   `npx -y rea-agents@latest inspect-apple-ips Coucou-2026-10-28-123456.ips --format md`
   Ele mostra o tipo da falha, a thread que caiu e as funções em que ela estava.
3. **Um nome de função Swift ilegível** (`$s5Coucou…`): `npx -y rea-agents@latest demangle-swift '$s5Coucou…'`.
4. **As bibliotecas que o app carrega**: descompacte o IPA (é um .zip) e rode
   `npx -y rea-agents@latest trace-dylib-resolution Payload/Coucou.app --format md`.

O CI faz o passo 1 a cada build (o artefato **REA-evidence** traz tudo). Os textos do Hermes ao iniciar ficam também em
**Ajustes → Privacidade e Segurança → Análise e Melhorias** (sysdiagnose) e, com o iPhone no cabo, no app Console do
Mac, filtrando por **Coucou**.
