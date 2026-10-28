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
- **Carteira**: *"como está a minha carteira hoje?"*, *"quanto está o GARE11?"*. Os preços da B3 vêm do Google
  Finance (com uns 15 minutos de atraso).
- **Memória**: o Hermes lembra do que você contar e das conversas anteriores, como no computador.

Cada coisa que o iPhone protege (calendário, lembretes, contatos, localização, notificações) o iOS pergunta na
primeira vez. **Ajustes → O que o AXYON pode usar** mostra o que está liberado.

## As telas

- **AXYON**: o orbe do Hermes (a cor diz o que ele está fazendo), atalhos *Faça por mim*, a carteira, o seu dia, os
  agentes do computador (com Permitir/Negar pelo Face ID) e o computador.
- **Chat**: a conversa com o Hermes, com cada passo que ele dá (lendo a carteira, agendando, pesquisando…), parar,
  ditado e leitura em voz alta.
- **Casa**: a casa do computador pareado, quando há um.
- **Carteira**: total, o dia, desde a compra, cada posição com o seu preço teto e os ativos que você acompanha.
- **Ajustes**: o cérebro, as permissões, o computador (opcional) e o resto dos ajustes do app.

## Com um computador (opcional)

**Ajustes → Computador (opcional) → Computador Windows ou Linux** pareia como antes (código ou QR). Com ele pareado:

- a **carteira** do AXYON do computador (posições, os seus tetos, os últimos preços e os avisos da aba do AXYON) passa
  para o iPhone sempre que ele se conecta, e fica guardada: com o computador desligado, o Hermes do iPhone continua
  respondendo sobre ela, com os preços de agora;
- a **casa** do computador aparece na aba Casa e o Hermes pode ligar e desligar os aparelhos dela;
- os **agentes** do computador aparecem no início, com Permitir/Negar;
- o **chat com o AXYON do computador** e os ajustes do assistente de lá continuam em Ajustes.

Fora de casa, use o Tailscale nos dois ([IPHONE_PC.pt-BR.md](IPHONE_PC.pt-BR.md)).

## Limites do iOS

- O iOS suspende os apps pouco depois de saírem da tela: o Hermes pensa com o AXYON aberto. Uma resposta em andamento
  ganha o meio minuto que o iOS dá para terminar, e uma notificação traz a resposta.
- O iOS não deixa um app rodar outros programas: as ferramentas de terminal e de código do Hermes não existem no
  iPhone (no computador, sim).
- Se o Hermes não iniciar, o início mostra o motivo; feche o AXYON no seletor de apps e abra de novo.
