# Zheus One (Windows e Linux)

*English: [ZHEUS_ONE.md](ZHEUS_ONE.md)*

"Zheus, estou indo para casa. Deixa tudo pronto e me conta o que aconteceu hoje." Com o **Zheus One**, essa frase é uma
missão, não quinze comandos: o seu assistente olha a casa inteira, monta um plano — o ar-condicionado ligado na hora
certa, o aspirador terminando antes de você chegar, as luzes depois de escurecer, o resumo do seu dia pronto — mostra
para você com o que conflita, e começa quando você diz sim.

O Zheus One fica em cima de duas coisas que você já tem no Coucou: a **Casa do Mochi** ([SMART_HOME.pt-BR.md](SMART_HOME.pt-BR.md)),
os aparelhos da sua casa, e **o seu assistente** ([HERMES.pt-BR.md](HERMES.pt-BR.md)), o Hermes ou o provedor do chat.
Ele usa só os aparelhos que você compartilha na Casa do Mochi, pelas mesmas checagens e o mesmo registro. Desligado por
padrão.

## Ligando

1. Ligue a **Casa do Mochi** e marque os aparelhos que o Zheus pode usar (Configurações → Casa do Mochi). O aspirador,
   o ar-condicionado e os sensores vêm do Home Assistant (2026.3 ou mais novo para limpar cômodo por cômodo).
2. Configurações → **Zheus One** (em vidro, como a ilha): ligue e ajuste os cartões. **Sua chegada**: quanto tempo o
   caminho de casa costuma levar, a temperatura do ar e quantos minutos antes de você chegar ele liga, e se o resumo do
   dia é contado na chegada. **Aspirador**: se ele limpa antes de você chegar, quanto tempo leva a casa inteira, a
   bateria mínima e os cômodos que ele limpa com os minutos de cada um (na ordem de importância). **Luzes e TV**: a
   partir de que hora está escuro, as luzes para uma chegada depois de escurecer, a TV e um app.
3. Aparece uma aba **✦** na ilha: a tela do Zheus One.

## A missão da chegada

Diga do jeito que quiser, no chat da ilha, no celular (o chat com o computador, por voz ou texto), ou deixe o Hermes
começar: "estou indo para casa", "tô indo pra casa, chego em 25 minutos", "voltando pra casa em meia hora". O próprio
Coucou lê a frase — não um modelo — com os minutos quando você diz, senão o seu tempo de costume.

**Foresight** (previsão): antes de qualquer coisa rodar, o Zheus mostra o plano, passo a passo, com os horários:

- **O ar-condicionado** liga quantos minutos antes de você chegar você definiu, não antes: nada de resfriar casa vazia.
- **O aspirador** limpa para terminar antes de você chegar. Quando o caminho é mais curto que a limpeza, o Zheus avisa
  e oferece saídas: *limpar só o quarto agora (cerca de 15 min) e a sala outra hora*, *começar agora e mandar voltar
  para a base pouco antes de você chegar*, ou *sem limpeza hoje*. Ele fica na base quando tem alguém em casa, quando a
  bateria está baixa ou quando precisa de ajuda, e o plano diz o porquê.
- **As luzes** que você escolheu (ou as da sala) acendem na sua chegada depois de escurecer; **a TV** e o app, se você
  pediu.
- **O resumo do dia** é preparado pelo seu assistente alguns minutos antes de você chegar, com as ferramentas e os
  conectores que ele tem (agenda, tarefas, mensagens, negócios), só leitura.
- **As boas-vindas**: na chegada, o Zheus diz o que foi feito e como foi o seu dia, na ilha e em voz alta quando a Voz
  está ligada: "Bem-vindo de volta! Feito: … Quer começar pelos negócios ou relaxar primeiro?"

Nada roda até você dizer sim. Responda **"sim"** ("pode", "ok", "manda ver") no mesmo chat, no celular ou no
computador, ou clique em **Aprovar** na ilha. "Pode, mas sem limpeza hoje", "manda voltar pra base", "só o quarto"
escolhem uma saída para o conflito ao aprovar; "não" ou "cancela" descartam o plano, ou param o que está rodando. Só
vale uma resposta curta, e só enquanto um plano espera: palavras que também significam outra coisa ("para", "deixa",
"no") nunca cancelam um plano. Um passo num aparelho que pede confirmação continua esperando o próprio clique, na ilha
ou no celular (Face ID).

A sua mensagem continua indo para o assistente, com o plano junto, então o resto dela ("…e me conta como foi o dia",
"me lembra de comprar pão") também é respondido, e o assistente sabe que não deve mexer nesses aparelhos ele mesmo.

## O World State (o estado da casa)

A tela ✦ mostra a casa em cartões de vidro: o aspirador (limpando, na base, a bateria), o ar-condicionado (a
temperatura dele, a do ambiente, − e +), as luzes (quantas estão acesas) e a TV, cada um com a sua chave (a do
aspirador manda limpar, ou voltar para a base); um tipo de aparelho que você ainda não compartilhou guarda o lugar
num cartão vazio, que abre as Configurações; onde você está (a caminho, em casa, fora); os fatos que ligam as coisas
("a TV está ligada onde o aspirador está limpando", "não tem ninguém em casa, e isto está ligado"); e as missões:
**Chegada 15 · 30 · 45 · 60 min** (preparar minha chegada nesses minutos) e **Situação** para a TV. Quem está em casa
vem das pessoas do Home Assistant (o app companion no seu celular, o seu roteador), ou do que você disse ("cheguei").

## O Autopilot

Objetivos permanentes em vez de comandos repetidos, em Configurações → Zheus One → Autopilot. Cada objetivo fica
**Desligado**, **Sugerir** ou **Agir** (dentro dos limites logo abaixo dele); o Zheus olha a casa a cada cinco minutos
enquanto algum está ligado, e nada roda quando estão todos desligados.

- **Menos desperdício de energia**: o que ficou ligado (luzes, TV, ar — nunca uma tomada) enquanto não tem ninguém em
  casa há os minutos que você definiu.
- **Casa sempre organizada**: o aspirador limpa a casa vazia uma vez por dia, no horário dele, e volta para a base
  quando alguém chega.
- **Seu dia preparado**: de manhã, o resumo da sua agenda, tarefas e mensagens, feito pelo assistente, só leitura.
- **Negócios evoluindo**: o assistente olha suas campanhas e vendas (com os conectores e as skills que você deu a ele:
  Instagram, Meta Ads, sua loja) e prepara propostas. **Só sugestões, sempre.**

O que nunca é feito sozinho, diga o objetivo o que disser: gastar, publicar, escrever para pessoas (clientes
inclusive) e qualquer coisa da segurança da casa; e uma ação num aparelho que pede confirmação sempre espera o seu
clique. Uma sugestão aparece na ilha com **Sim** e **Não**, uma vez por dia cada.

## O Modo TV

"Zheus, mostra nossa situação" (ou **Situação** na ilha, ou **Abrir na TV** nas Configurações): a casa, o plano em
andamento, as decisões que esperam por você e o panorama falado do seu assistente, em tela cheia e em vidro — na TV ou
no projetor ligado ao computador (ou espelhado nele) quando houver um. Esc fecha. Ele lê a casa a cada meio minuto
enquanto está na tela, e nada depois de fechado.

## O visual: Liquid Glass

Configurações → Zheus One → **Visual da ilha**: **Liquid Glass** (luz nas bordas, os cartões foscos sobre um campo
suave de cor) ou **Clássica** (a ilha preta). O vidro é desenhado pela página, não pela janela (a ilha mora numa janela
fixa e transparente, e desfocar a área de trabalho foscaria o retângulo inteiro), por isso o tom dele é escuro o
bastante para a página atrás da ilha nunca aparecer através dela. Nada nele se mexe: não custa nada enquanto a ilha
está escondida.

## Para o Hermes e outros agentes

Com a Casa do Mochi compartilhada com o Hermes (Configurações → Hermes Agent), o `coucou-hook --mcp` também oferece
**zheus_world** (o estado da casa em palavras) e **zheus_mission** (`arrival`, com os minutos, ou `situation`). Uma
missão pedida por um agente só mostra o plano: é o seu sim que o faz rodar, no Coucou.

## O que saber

- Testado até agora com aparelhos simulados e um Home Assistant simulado, não com um aspirador ou um ar-condicionado
  de verdade. O que acontece num PC Windows de verdade (o Modo TV numa segunda tela, a voz) é para você testar.
- Um plano esperando ou rodando vive no Coucou: se o Coucou fechar, ele é descartado.
- O Zheus não tem localização própria: "a caminho" é o que você diz, "em casa" são as pessoas do Home Assistant ou o
  que você diz.
- As temperaturas são lidas em °C.
- O próprio Coucou não se conecta ao Instagram, ao Meta Ads, ao Google Agenda nem ao Drive: isso passa pelos
  conectores e skills do seu assistente.
