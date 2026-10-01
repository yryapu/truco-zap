#!/usr/bin/env bash
# Prova de fumaça da API HTTP, do terminal. Usada como `prova` em resultado.json.
# Uso: scripts/fumaca.sh [base_url]   (padrão http://localhost:18080)
set -euo pipefail
BASE="${1:-http://localhost:18080}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
ok=0; falhou=0
checa() { # checa <descricao> <esperado> <obtido>
  if [ "$2" = "$3" ]; then printf '  ok   %s\n' "$1"; ok=$((ok+1));
  else printf '  FALHOU %s — esperava %s, veio %s\n' "$1" "$2" "$3"; falhou=$((falhou+1)); fi
}
jq_() { python3 -c "import sys,json;d=json.load(sys.stdin);print(eval('d'+sys.argv[1]))" "$1"; }

echo "== saúde =="
checa "/api/saude responde ok" "True" "$(curl -sS "$BASE/api/saude" | jq_ "['ok']")"

echo "== cadastro rápido =="
curl -sS -X POST "$BASE/api/convidado" -c "$TMP/a" -o "$TMP/a.json"
checa "convidado começa com 1000 moedas" "1000" "$(jq_ "['jogador']['saldo']" < "$TMP/a.json")"
checa "convidado já tem emblema" "estreante" "$(jq_ "['jogador']['emblemas'][0]['chave']" < "$TMP/a.json")"
checa "cookie é HttpOnly" "1" "$(grep -c HttpOnly "$TMP/a" || true)"

echo "== sessão isolada =="
for rota in /api/eu /api/historico /api/webhooks; do
  checa "$rota sem cookie = 401" "401" "$(curl -sS -o /dev/null -w '%{http_code}' "$BASE$rota")"
done
checa "/api/eu com cookie = 200" "200" "$(curl -sS -b "$TMP/a" -o /dev/null -w '%{http_code}' "$BASE/api/eu")"
checa "cookie forjado = 401" "401" \
  "$(curl -sS -o /dev/null -w '%{http_code}' -H "cookie: truco_sessao=$(printf 'A%.0s' {1..43})" "$BASE/api/eu")"

echo "== guarda de SSRF no registro de webhook =="
# Só as checagens que valem em QUALQUER configuração. A pilha do docker-compose liga
# TRUCO_WEBHOOK_ALLOW_HTTP e _ALLOW_PRIVATE para o receptor de teste, então `http://` e
# endereço privado são aceitos aqui de propósito — e eu não vou fingir que testei o que não
# testei. A recusa de http, loopback, link-local, privado, CGNAT e IPv4-mapeado está provada
# em `cargo test -p truco_server webhooks` (testes recusa_esquema_e_loopback e
# reconhece_enderecos_internos), que rodam com a configuração de produção.
for u in "file:///etc/passwd" "gopher://exemplo.com/h" "nao-e-url"; do
  r=$(curl -sS -b "$TMP/a" -X POST "$BASE/api/webhooks" -H 'content-type: application/json' \
        -d "{\"url\":\"$u\"}" | python3 -c 'import sys,json;print(json.load(sys.stdin).get("erro","sem-erro"))')
  checa "webhook recusa $u" "url_recusada" "$r"
done

echo "== ranking é público =="
checa "/api/ranking = 200" "200" "$(curl -sS -o /dev/null -w '%{http_code}' "$BASE/api/ranking")"

echo
echo "$ok ok, $falhou falhou"
[ "$falhou" -eq 0 ]
