# Coucou com controle pelo VPS

## Resultado

O Coucou fica dividido em duas partes, porque o componente que toca o Windows precisa continuar no próprio notebook:

```text
iPhone / Android
       |
       | WSS + criptografia ponta a ponta do pareamento
       v
VPS: Nginx TLS -> Coucou relay (sem estado, sem conteúdo legível)
       ^
       | WebSocket de saída; nenhuma porta aberta no roteador de casa
       |
Notebook: Coucou + hooks + Braços do Mochi
       |
       +-- arquivos, comandos e aplicativos locais (sempre pelas regras de aprovação)
       +-- Codex / Claude / outros agentes
```

Opcionalmente, o VPS também hospeda o **Hermes Agent**. Nesse caso memória, agenda, automações, integrações e tarefas
agendadas ficam no VPS, e o Coucou do notebook se conecta ao gateway Hermes por HTTPS. Os Braços do Mochi continuam no
Windows: mover essa parte para o VPS faria o servidor agir no próprio VPS, não no notebook.

## Limite atual do iPhone

O cliente Android deste repositório já implementa o transporte `relay`. O cliente iPhone desta branch ainda ignora os
campos de relay do QR code e usa a conexão direta (Wi-Fi local ou **Fora de casa, direto**). Portanto:

- Android: usa este relay imediatamente;
- iPhone: continua funcionando diretamente; para usar o VPS como caminho obrigatório é necessário adicionar o transporte
  relay ao `PCLink` em Swift ou usar uma VPN privada entre o iPhone e o notebook.

Não esconda essa diferença trocando apenas o servidor: o QR contém uma chave de ponta a ponta que o cliente precisa
entender. Um proxy HTTP comum não substitui o transporte do aplicativo.

## Componentes do VPS

- `android/relay/compose.vps.yml`: serviço sem privilégios, filesystem somente leitura, sem volume de dados e exposto só
  em `127.0.0.1:8787`;
- `android/relay/tools/vps-relay.ts`: entrada de produção; exige uma chave base64url de 43 caracteres e falha fechada;
- `android/relay/deploy/nginx-coucou-relay.conf`: termina TLS e encaminha somente o healthcheck e salas WebSocket válidas;
- `android/relay/deploy/vps-up.sh`: gera o segredo com permissão 0600 e sobe/valida o serviço.

O relay não grava banco, fila, mensagens ou arquivos. Ele vê IPs, horário e tamanho acolchoado dos frames; não vê
sessões, comandos, caminhos, respostas, tokens de pareamento nem as mensagens do chat.

## Ordem segura de implantação

1. Criar um DNS exclusivo, por exemplo `coucou-relay.seudominio.com`, apontando para o VPS.
2. No VPS, executar `android/relay/deploy/vps-up.sh`.
3. Emitir o certificado TLS e habilitar o bloco Nginx com `access_log off`.
4. Confirmar que `https://coucou-relay.seudominio.com/` responde `Coucou link relay`.
5. No Windows: **Configurações -> Celular -> Fora de casa por Wi-Fi**; salvar `wss://...` e a chave do arquivo
   `android/relay/secrets/access-key`; então ligar o relay.
6. Gerar um novo QR code. O novo pareamento cria uma sala e uma chave ponta a ponta novas.
7. Testar primeiro **Negar**, depois uma ação inofensiva que pede **Permitir**. Nunca comece por comando destrutivo.

## Regras de segurança

- O notebook só abre conexões de saída; não habilitar UPnP nem publicar a porta 47821 quando o relay estiver em uso.
- A chave de acesso do relay protege uso/cota. A chave de pareamento, diferente e nunca vista pelo VPS, protege o conteúdo.
- Perdeu o celular: use **Parear de novo** no notebook. Isso invalida a sala e a chave ponta a ponta antigas.
- Vazou a chave do relay: gere outra no VPS e salve-a no Coucou; os telefones conectados recebem a rotação dentro do canal
  criptografado.
- O serviço deve continuar atrás de TLS; nunca publique `ws://` na internet.
- O acesso remoto não remove as confirmações do Coucou. Aprovação ausente expira; nunca vira permissão automática.
