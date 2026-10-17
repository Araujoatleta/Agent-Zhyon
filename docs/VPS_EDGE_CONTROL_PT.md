# Operação e modelo de ameaças — controle VPS e worker Coucou (MVP)

## Estado implementado

O Hermes existente (`0.0.0.0:9119`) continua sendo o cérebro e não é reinstalado. O `vps-control` escuta `127.0.0.1:8790`, persiste dispositivos, pareamentos, tarefas, limites, nonces e auditoria no volume Docker e pode ser publicado pelo Nginx em `https://coucou-relay.navellux.com.br/control/`. O relay existente em `8787`, `/` e `/v1/room` não muda.

O worker Windows faz apenas conexões HTTPS/WSS de saída. Ele sempre anuncia `health` e `system_info`; anuncia `computer_use` somente quando encontra a ponte CUA na sessão interativa. Não há shell remoto. O worker mantém um ledger local de idempotência em `%LOCALAPPDATA%\Coucou\edge-worker\<namespace>\task-ledger.jsonl`.

## Empacotamento Windows

`npm run pack` compila `coucou-hook` e `coucou-edge-worker` com `Cargo.lock`, gera NSIS e MSI, cria o diretório portátil com `Coucou.exe`, `coucou-hook.exe` e `coucou-edge-worker.exe`, grava `release/SHA256SUMS.txt` e verifica cada SHA-256 imediatamente. O CI executa novamente `npm run verify:artifacts`. Os artifacts do Actions são identificados como `unsigned`; assinatura não é simulada quando os secrets de assinatura não existem.

NSIS instala `coucou-edge-worker.exe` como recurso do aplicativo e cria a tarefa **Coucou Edge Worker** com gatilho **ONLOGON**, privilégio `LIMITED` e execução na sessão do usuário. A tarefa fica **desabilitada** por padrão: instalação não equivale a consentimento nem a pareamento. O uninstall do NSIS remove a tarefa antes de apagar os arquivos. MSI e pacote portátil contêm o worker, mas não criam autostart. Não use serviço, `SYSTEM` ou Session 0.

Para conferir um download no PowerShell, no diretório que contém o arquivo e o manifesto:

```powershell
(Get-FileHash .\Coucou-Windows-0.4.0-setup.exe -Algorithm SHA256).Hash.ToLower()
Select-String .\SHA256SUMS.txt -Pattern 'Coucou-Windows-0.4.0-setup.exe'
```

## Deploy do VPS

No checkout `/opt/agent-zhyon-relay`, integre a revisão sem substituir a configuração do relay. Crie `vps-control/.env` com modo `0600` e um `ADMIN_TOKEN` aleatório de pelo menos 24 caracteres, por exemplo `openssl rand -hex 32`. Então:

```sh
docker compose -f vps-control/compose.yml --project-directory vps-control up -d --build
```

Copie apenas o bloco de `vps-control/deploy/nginx-control.conf` para o `server` de `coucou-relay.navellux.com.br`, execute `nginx -t && systemctl reload nginx` e valide:

```sh
curl -f https://coucou-relay.navellux.com.br/control/healthz
curl -f https://coucou-relay.navellux.com.br/
```

A segunda chamada deve continuar respondendo pelo relay antigo. O unit systemd fornecido é opcional. Não use `desktop.navellux.com.br`, já ocupado em `6080`.

## Pareamento implementado

1. Defina variáveis no usuário que executará o worker:

   ```powershell
   [Environment]::SetEnvironmentVariable('COUCOU_VPS_URL', 'https://coucou-relay.navellux.com.br/control', 'User')
   [Environment]::SetEnvironmentVariable('COUCOU_DEVICE_NAME', 'Notebook casa', 'User')
   ```

2. Execute `coucou-edge-worker.exe` manualmente nessa sessão. Sem identidade armazenada, ele chama `POST /v1/pair/start` com nome, chave pública Ed25519 e capabilities. O servidor devolve `pairingId`, PIN de seis dígitos, `claimToken` e expiração de cinco minutos. O worker mostra o PIN e consulta `GET /v1/pair/status/PAIRING_ID` a cada dois segundos por até dez minutos.
3. A aprovação implementada é `POST /v1/admin/pairings/PAIRING_ID/approve` com `Authorization: Bearer <ADMIN_TOKEN>`.
4. Após a aprovação, o worker recebe `deviceId` e credential uma única vez e guarda identidade, credential e chave privada pelo `keyring` no Windows Credential Manager, em namespace derivado da origem do servidor.
5. Só depois de confirmar o dispositivo em `GET /v1/admin/devices`, habilite e inicie a tarefa:

   ```powershell
   schtasks.exe /Change /TN "Coucou Edge Worker" /ENABLE
   schtasks.exe /Run /TN "Coucou Edge Worker"
   ```

**Lacuna operacional atual:** o worker imprime o PIN, mas não imprime o `pairingId`; a API admin não lista pareamentos pendentes e não converte PIN em `pairingId`. Portanto, o fluxo sem acesso ao estado interno ainda não é operável de ponta a ponta. Não habilite a tarefa em rollout geral até existir UI/endpoint autenticado para resolver o PIN, ou até o worker expor com segurança o `pairingId`. `COUCOU_PAIR_PIN` não é lido pela implementação atual.

## Tarefas e revogação

Liste dispositivos com `GET /v1/admin/devices`. `POST /v1/admin/tasks` exige `deviceId`, `capability`, `args`, `idempotencyKey`, `deadlineAt` futuro, `timeoutMs`, `approvalClass`, `summary`, `maxOutputBytes` e `artifactPolicy`; HTTP 202 significa somente enfileirado. Confirme pelo `GET /v1/admin/tasks/TASK_ID` até estado terminal. Cancelamento existe em `POST /v1/admin/tasks/TASK_ID/cancel`.

Revogue com:

```sh
curl -X POST -H "Authorization: Bearer $ADMIN_TOKEN" \
  https://coucou-relay.navellux.com.br/control/v1/admin/devices/DEVICE_ID/revoke
```

A revogação fecha sockets ativos e impede nova autenticação. Para parear novamente, também exclua no Credential Manager a entrada `identity` do serviço `Coucou Edge Worker <hash-da-origem>` correspondente.

## Modelo de ameaças e controles

- O PIN tem seis dígitos, validade padrão de cinco minutos e o início de pareamento é limitado por origem a dez tentativas por minuto. O limite é persistido no arquivo local, não é distribuído entre réplicas.
- O servidor persiste hashes SHA-256 do PIN, claim token, credential e nonces; o `claimToken` e a credential são entregues ao cliente. O arquivo é criado com modo `0600` no Linux.
- Cada abertura de WSS exige credential, timestamp recente, nonce não reutilizado e assinatura Ed25519 sobre `deviceId.timestamp.nonce`. O relógio do host precisa estar sincronizado.
- Corpos HTTP e frames WSS são limitados; capabilities e campos são validados; idempotency keys evitam colisões divergentes; o worker limita evidência local a 64 KiB.
- TLS termina no Nginx. Use HTTPS/WSS. HTTP/WS só é aceito pelo worker para loopback quando `COUCOU_ALLOW_INSECURE_LOCALHOST=1`.
- O token admin deve ficar em secret/variável fora de prompts, logs e repositório. A auditoria não deve ser usada para armazenar segredos.
- `computer_use` depende de ponte CUA local e roda no usuário interativo. A presença da capability não substitui aprovação humana para senhas, permissões, pagamentos ou ações destrutivas.

## Rollout seguro

1. Faça backup do volume, suba o control plane e valide health, TLS e o relay antigo.
2. Teste um único dispositivo, manualmente e com a tarefa ainda desabilitada.
3. Resolva a lacuna PIN → `pairingId`; aprove, confirme o dispositivo e execute apenas `health`/`system_info`.
4. Habilite ONLOGON nesse canário, reinicie a sessão e confirme reconexão, idempotência, cancelamento e revogação.
5. Só então amplie gradualmente. Guarde `SHA256SUMS.txt`, revisão implantada e resultado dos checks do CI como evidência.
6. Para rollback, desabilite/remova a tarefa, revogue o dispositivo e restaure o container/volume anterior; o relay existente permanece separado.

## Diagnóstico e limitações

Use `docker compose logs --tail=100 coucou-control`, `curl http://127.0.0.1:8790/healthz`, `nginx -t`, o Agendador de Tarefas e o relógio do host. HTTP/WSS 401 costuma indicar credential, prova, nonce, horário ou revogação inválidos. O reconnect usa backoff limitado a 60 segundos.

Limitações do MVP: lacuna de aprovação por PIN descrita acima; sem dashboard; sem HA/locking compartilhado; store JSON não é adequado a cluster; sem rotação automática de credential/chave; sem atualização automática; builds Windows não assinados quando secrets não estão disponíveis; MSI não gerencia a tarefa; portable não gerencia autostart; automação GUI depende de CUA local e não foi validada por esta integração de empacotamento.
