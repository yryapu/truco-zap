// truco-zap — cliente. JavaScript puro, sem build step (decisão D06: WASM não compra nada aqui).
//
// Nenhuma regra de truco vive aqui. O cliente desenha o que o servidor manda e manda de
// volta a intenção do jogador. Os botões ligam/desligam pela lista `opcoes` que vem no
// estado — é por isso que o botão de truco simplesmente não existe na mão de onze (D10).
'use strict';

const $ = (id) => document.getElementById(id);
const BASE_NAIPE = { 0x1f0a0: 'espadas', 0x1f0b0: 'copas', 0x1f0c0: 'ouros', 0x1f0d0: 'paus' };
const COSTAS = '\u{1F0A0}';

let ws = null;
let modo = '1x1';
let estado = null;
let meuPerfil = null;

function naipeDe(ch) {
  const cp = ch.codePointAt(0);
  return BASE_NAIPE[cp & 0xfffffff0] || null;
}

function elCarta(ch, { manilha = false, pequena = false } = {}) {
  const e = document.createElement('span');
  e.className = 'carta' + (pequena ? ' pequena' : '');
  e.textContent = ch;
  if (ch === COSTAS) {
    e.classList.add('costas');
    e.title = 'carta encoberta';
  } else {
    const n = naipeDe(ch);
    if (n === 'copas' || n === 'ouros') e.classList.add('vermelha');
    if (manilha) e.classList.add('manilha');
    e.title = n ? `${ch} de ${n}` : ch;
  }
  return e;
}

async function api(rota, opcoes = {}) {
  const r = await fetch(rota, {
    credentials: 'same-origin',
    headers: { 'content-type': 'application/json' },
    ...opcoes,
  });
  const corpo = await r.json().catch(() => ({}));
  if (!r.ok) throw new Error(corpo.erro || `http_${r.status}`);
  return corpo;
}

// ───────────────────────────────── telas

function mostrar(tela) {
  for (const t of ['tela-entrada', 'tela-lobby', 'tela-mesa']) $(t).hidden = t !== tela;
}

function pintarPerfil(j) {
  meuPerfil = j;
  $('barra-perfil').hidden = false;
  $('perfil-apelido').textContent = j.apelido;
  $('perfil-saldo').textContent = j.saldo;
  $('perfil-placar').textContent = `${j.vitorias}V / ${j.derrotas}D`;
  const alvo = $('emblemas');
  alvo.textContent = '';
  for (const e of j.emblemas) {
    const d = document.createElement('div');
    d.className = 'emblema';
    d.dataset.chave = e.chave;
    d.innerHTML = `<span class="ic"></span><span class="nm"></span>`;
    d.querySelector('.ic').textContent = e.icone;
    d.querySelector('.nm').textContent = `${e.nome} — ${e.motivo}`;
    alvo.appendChild(d);
  }
}

async function carregarLobby() {
  mostrar('tela-lobby');
  try {
    const { ranking } = await api('/api/ranking');
    const t = $('ranking');
    t.textContent = '';
    for (const l of ranking) {
      const tr = document.createElement('tr');
      for (const v of [l.posicao, l.apelido, `${l.vitorias}V / ${l.derrotas}D`, `${l.saldo} 🪙`]) {
        const td = document.createElement('td');
        td.textContent = v;
        tr.appendChild(td);
      }
      t.appendChild(tr);
    }
    if (!ranking.length) t.innerHTML = '<tr><td colspan="4">ninguém jogou ainda</td></tr>';
  } catch (_) {}
  try {
    const { historico } = await api('/api/historico');
    const t = $('historico');
    t.textContent = '';
    for (const h of historico) {
      const tr = document.createElement('tr');
      const r = h.venceu ? `ganhou +${h.premio}` : `perdeu -${h.aposta}`;
      for (const v of [h.modo, `${h.placar_time0}×${h.placar_time1}`, r]) {
        const td = document.createElement('td');
        td.textContent = v;
        tr.appendChild(td);
      }
      t.appendChild(tr);
    }
    if (!historico.length) t.innerHTML = '<tr><td colspan="3">nenhuma partida terminada</td></tr>';
  } catch (_) {}
  carregarWebhooks();
}

async function carregarWebhooks() {
  try {
    const { webhooks } = await api('/api/webhooks');
    const ul = $('webhooks');
    ul.textContent = '';
    for (const w of webhooks) {
      const li = document.createElement('li');
      li.dataset.id = w.id;
      const span = document.createElement('span');
      span.textContent = w.url + ' ';
      const b = document.createElement('button');
      b.textContent = 'remover';
      b.className = 'fantasma';
      b.onclick = async () => {
        await api(`/api/webhooks/${w.id}`, { method: 'DELETE' });
        carregarWebhooks();
      };
      li.append(span, b);
      ul.appendChild(li);
    }
  } catch (_) {}
}

// ───────────────────────────────── WebSocket

function conectar() {
  const esquema = location.protocol === 'https:' ? 'wss' : 'ws';
  ws = new WebSocket(`${esquema}://${location.host}/ws`);
  ws.onmessage = (ev) => receber(JSON.parse(ev.data));
  ws.onclose = () => log('conexão caiu — recarregue a página');
  return new Promise((ok, falha) => {
    ws.onopen = ok;
    ws.onerror = falha;
  });
}

function manda(obj) {
  if (ws && ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify(obj));
}

function log(txt) {
  const li = document.createElement('li');
  li.textContent = txt;
  $('log').prepend(li);
}

const NOME_ERRO = {
  nao_eh_sua_vez: 'não é a sua vez',
  carta_nao_esta_na_sua_mao: 'essa carta não está na sua mão',
  encoberta_na_primeira_rodada: 'não dá para jogar de costas na primeira rodada',
  truco_proibido_mao_de_onze: 'não se pede truco na mão de onze',
  seu_time_fez_o_ultimo_pedido: 'quem pede agora é a outra dupla',
  valor_maximo_atingido: 'a mão já vale 12',
  aposta_invalida: 'aposta acima do seu saldo',
  saldo_insuficiente: 'saldo insuficiente para essa aposta',
  ja_esta_em_partida: 'você já está numa partida',
};

function receber(m) {
  switch (m.t) {
    case 'ola':
      pintarPerfil(m.jogador);
      break;
    case 'fila':
      $('status-fila').textContent =
        m.faltam > 0
          ? `na fila ${m.modo} por ${m.aposta} moedas — faltam ${m.faltam}. ` +
            `Se ninguém aparecer em ${m.robos_em_segundos}s, a mesa é completada com robôs.`
          : 'montando mesa…';
      break;
    case 'fila_saiu':
      $('status-fila').textContent = 'saiu da fila';
      break;
    case 'mesa':
      $('caixa-fim').hidden = true;
      $('log').textContent = '';
      $('bolo').textContent = m.bolo;
      $('modo-mesa').textContent = `${m.modo} · aposta ${m.aposta}`;
      mostrar('tela-mesa');
      log(`mesa aberta (${m.modo}, aposta ${m.aposta}, bolo ${m.bolo})`);
      break;
    case 'estado':
      estado = m;
      pintarMesa(m);
      break;
    case 'evento':
      narrar(m.evento);
      break;
    case 'erro':
      $('erro-mesa').textContent = NOME_ERRO[m.codigo] || m.codigo;
      $('status-fila').textContent = NOME_ERRO[m.codigo] || m.codigo;
      setTimeout(() => ($('erro-mesa').textContent = ''), 3500);
      break;
    case 'fim':
      $('caixa-fim').hidden = false;
      $('fim').textContent = m.venceu
        ? `Vitória! ${m.placar[0]}×${m.placar[1]} · +${m.premio} moedas`
        : `Derrota. ${m.placar[0]}×${m.placar[1]}`;
      if (m.saldo != null && meuPerfil) {
        meuPerfil.saldo = m.saldo;
        $('perfil-saldo').textContent = m.saldo;
      }
      break;
    case 'derrubado':
      log('outra aba assumiu esta conta');
      break;
  }
}

const NOME_TIPO = {
  normal: '',
  mao_de_onze: 'MÃO DE ONZE — vale 3, sem truco',
  mao_de_ferro: 'MÃO DE FERRO — quem ganhar, ganha a partida',
};

function narrar(e) {
  const quem = (a) => (estado?.jogadores?.[a]?.apelido ?? `assento ${a}`);
  switch (e.evento) {
    case 'mao_comecou':
      log(`mão ${e.mao}: vira ${e.vira} — manilha é o rank de ${e.manilha_rank}. Vale ${e.valor}.`);
      break;
    case 'carta_jogada':
      log(`${quem(e.assento)} jogou ${e.carta}${e.encoberta ? ' (encoberta)' : ''}`);
      break;
    case 'rodada_terminou':
      log(
        `rodada ${e.indice + 1}: ` +
          (e.vencedor === null ? 'empatou' : `time ${e.vencedor === 0 ? 'nós/eles 0' : '1'} levou`)
      );
      break;
    case 'pedido':
      log(`${quem(e.assento)} pediu ${e.proposto === 3 ? 'TRUCO' : e.proposto}!`);
      break;
    case 'pedido_aceito':
      log(`aceito — a mão agora vale ${e.valor}`);
      break;
    case 'correu':
      log(`time ${e.time_que_correu} correu — ${e.pontos} ponto(s) para o outro lado`);
      break;
    case 'mao_de_onze_recusada':
      log(`mão de onze recusada — 1 ponto para o time ${e.pontos_para}`);
      break;
    case 'mao_terminou':
      log(
        e.vencedor === null
          ? 'mão empatada — ninguém pontua'
          : `mão para o time ${e.vencedor}: +${e.pontos}. Placar ${e.placar[0]}×${e.placar[1]}`
      );
      break;
    case 'partida_terminou':
      log(`fim de partida — time ${e.vencedor} venceu ${e.placar[0]}×${e.placar[1]}`);
      break;
  }
}

function pintarMesa(s) {
  const meuTime = s.time;
  $('placar-nos').textContent = s.placar[meuTime];
  $('placar-eles').textContent = s.placar[1 - meuTime];
  $('valor-mao').textContent = s.valor;
  $('modo-mesa').textContent = `${s.modo} · aposta ${s.aposta}`;

  $('vira').textContent = '';
  $('vira').appendChild(elCarta(s.vira, { pequena: true }));
  const rankManilha = (() => {
    // A manilha é o rank imediatamente acima da vira (R3). O servidor já manda esse rank
    // no evento; aqui derivo só para o rótulo, a partir da carta mais forte conhecida.
    const ordem = ['4', '5', '6', '7', 'Q', 'J', 'K', 'A', '2', '3'];
    const off = s.vira.codePointAt(0) & 0xf;
    const mapa = { 1: 'A', 2: '2', 3: '3', 4: '4', 5: '5', 6: '6', 7: '7', 0xb: 'J', 0xd: 'Q', 0xe: 'K' };
    const i = ordem.indexOf(mapa[off]);
    return i < 0 ? '?' : ordem[(i + 1) % 10];
  })();
  $('manilha').textContent = rankManilha;
  $('tipo-mao').textContent = NOME_TIPO[tipoChave(s.tipo_mao)] || '';

  const js = $('jogadores');
  js.textContent = '';
  for (const j of s.jogadores) {
    const d = document.createElement('div');
    d.className = 'j';
    d.dataset.assento = j.assento;
    if (j.assento === s.assento) d.classList.add('eu');
    if (s.vez === j.assento) d.classList.add('vez');
    if (!j.online) d.classList.add('off');
    d.textContent = `${j.apelido}${j.robo ? ' 🤖' : ''} · ${j.time === meuTime ? 'nós' : 'eles'} · ${j.cartas_na_mao}🂠`;
    js.appendChild(d);
  }

  const mesa = $('cartas-na-mesa');
  mesa.textContent = '';
  for (const c of s.mesa) {
    const w = document.createElement('div');
    w.style.textAlign = 'center';
    w.appendChild(elCarta(c.carta, { manilha: ehManilha(c.carta, rankManilha) }));
    const nm = document.createElement('div');
    nm.className = 'discreto';
    nm.textContent = s.jogadores[c.assento]?.apelido ?? '';
    w.appendChild(nm);
    mesa.appendChild(w);
  }

  const rod = $('rodadas');
  rod.textContent = '';
  s.rodadas.forEach((r, i) => {
    const e = document.createElement('span');
    e.textContent = `${i + 1}ª: ${r === null ? 'empate' : r === meuTime ? 'nós' : 'eles'}`;
    rod.appendChild(e);
  });

  const mao = $('minha-mao');
  mao.textContent = '';
  const podeJogar = s.opcoes.includes('jogar');
  (s.minha_mao || []).forEach((c, i) => {
    const b = document.createElement('button');
    b.className = 'carta' + (naipeDe(c) === 'copas' || naipeDe(c) === 'ouros' ? ' vermelha' : '');
    if (ehManilha(c, rankManilha)) b.classList.add('manilha');
    b.textContent = c;
    b.dataset.testid = `carta-${i}`;
    b.dataset.carta = c;
    b.disabled = !podeJogar;
    b.onclick = () => manda({ t: 'jogar', carta: c, encoberta: $('chk-encoberta').checked });
    mao.appendChild(b);
  });
  if (s.mao_do_parceiro) {
    const d = document.createElement('div');
    d.className = 'discreto';
    d.dataset.testid = 'mao-do-parceiro';
    d.textContent = `mão do parceiro: ${s.mao_do_parceiro.cartas.join(' ')}`;
    mao.appendChild(d);
  }

  $('encoberta-opcao').hidden = !(podeJogar && s.encoberta_permitida);
  if (!s.encoberta_permitida) $('chk-encoberta').checked = false;

  const ligar = (id, ligado, texto) => {
    $(id).hidden = !ligado;
    if (texto) $(id).textContent = texto;
  };
  const proximoValor = { 1: 'TRUCO (3)', 3: 'seis', 6: 'nove', 9: 'doze' }[s.valor] || '';
  ligar('btn-pedir', s.opcoes.includes('pedir'), `pedir ${proximoValor}`);
  ligar('btn-aceitar', s.opcoes.includes('aceitar'));
  ligar('btn-correr', s.opcoes.includes('correr'));
  ligar('btn-aumentar', s.opcoes.includes('aumentar'), `aumentar`);
  ligar('btn-onze-jogar', s.opcoes.includes('mao_de_onze_jogar'));
  ligar('btn-onze-correr', s.opcoes.includes('mao_de_onze_correr'));

  $('vez').textContent = s.pode_agir
    ? s.opcoes.includes('jogar')
      ? 'sua vez — escolha uma carta'
      : 'sua vez — responda'
    : s.vez != null
      ? `vez de ${s.jogadores[s.vez]?.apelido ?? '…'}`
      : 'aguardando…';
}

function tipoChave(t) {
  if (typeof t === 'string') return t;
  if (t && typeof t === 'object') return Object.keys(t)[0];
  return 'normal';
}

function ehManilha(ch, rankManilha) {
  if (ch === COSTAS) return false;
  const off = ch.codePointAt(0) & 0xf;
  const mapa = { 1: 'A', 2: '2', 3: '3', 4: '4', 5: '5', 6: '6', 7: '7', 0xb: 'J', 0xd: 'Q', 0xe: 'K' };
  return mapa[off] === rankManilha;
}

// ───────────────────────────────── eventos de UI

$('btn-convidado').onclick = async () => {
  $('erro-entrada').textContent = '';
  try {
    const { jogador } = await api('/api/convidado', { method: 'POST' });
    pintarPerfil(jogador);
    await conectar();
    await carregarLobby();
  } catch (e) {
    $('erro-entrada').textContent = e.message;
  }
};

async function credenciar(rota) {
  $('erro-entrada').textContent = '';
  try {
    const { jogador } = await api(rota, {
      method: 'POST',
      body: JSON.stringify({ apelido: $('campo-apelido').value, senha: $('campo-senha').value }),
    });
    pintarPerfil(jogador);
    await conectar();
    await carregarLobby();
  } catch (e) {
    $('erro-entrada').textContent =
      { apelido_em_uso: 'esse apelido já existe', senha_curta: 'senha de 6 caracteres ou mais',
        apelido_invalido: 'apelido de 2 a 20 letras, números, - ou _',
        credenciais_invalidas: 'apelido ou senha errados' }[e.message] || e.message;
  }
}
$('btn-cadastrar').onclick = () => credenciar('/api/cadastro');
$('btn-entrar').onclick = () => credenciar('/api/entrar');

$('btn-sair').onclick = async () => {
  await api('/api/sair', { method: 'POST' }).catch(() => {});
  location.reload();
};

for (const m of ['1x1', '2x2']) {
  $(`modo-${m}`).onclick = () => {
    modo = m;
    $('modo-1x1').classList.toggle('escolhido', m === '1x1');
    $('modo-2x2').classList.toggle('escolhido', m === '2x2');
  };
}

$('btn-fila').onclick = () => {
  $('status-fila').textContent = 'entrando na fila…';
  manda({ t: 'entrar_fila', modo, aposta: Number($('campo-aposta').value) });
};

$('btn-pedir').onclick = () => manda({ t: 'pedir' });
$('btn-aceitar').onclick = () => manda({ t: 'aceitar' });
$('btn-correr').onclick = () => manda({ t: 'correr' });
$('btn-aumentar').onclick = () => manda({ t: 'aumentar' });
$('btn-onze-jogar').onclick = () => manda({ t: 'mao_de_onze', jogar: true });
$('btn-onze-correr').onclick = () => manda({ t: 'mao_de_onze', jogar: false });
$('btn-voltar').onclick = async () => {
  const { jogador } = await api('/api/eu');
  pintarPerfil(jogador);
  await carregarLobby();
};

$('btn-webhook').onclick = async () => {
  $('webhook-segredo').textContent = '';
  try {
    const r = await api('/api/webhooks', {
      method: 'POST',
      body: JSON.stringify({ url: $('webhook-url').value }),
    });
    $('webhook-segredo').textContent = `segredo (guarde, só aparece agora): ${r.segredo}`;
    $('webhook-url').value = '';
    carregarWebhooks();
  } catch (e) {
    $('webhook-segredo').textContent = `recusado: ${e.message}`;
  }
};

// Sessão já existente (cookie): entra direto no lobby.
(async () => {
  try {
    const { jogador } = await api('/api/eu');
    pintarPerfil(jogador);
    await conectar();
    await carregarLobby();
  } catch (_) {
    mostrar('tela-entrada');
  }
})();
