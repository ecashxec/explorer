const getAddress = () => window.location.pathname.split('/')[2];

var isSatsTableLoaded = false;
function loadSatsTable() {
  if (!isSatsTableLoaded) {
    console.log(addrBalances[0].utxos);
    webix.ui({
      container: "sats-coins-table",
      view: "datatable",
      columns:[
        {
          id: "outpoint",
          header: "Outpoint",
          css: "hash",
          adjust: true,
          template: function (row) {
            return '<a href="/tx/' + row.txHash + '">' + 
              row.txHash + ':' + row.outIdx +
              (row.isCoinbase ? '<div class="ui green horizontal label">Coinbase</div>' : '') +
              '</a>';
          },
        },
        {
          id: "blockHeight",
          header: "Block Height",
          adjust: true,
          template: function (row) {
            return '<a href="/block-height/' + row.blockHeight + '">' + renderInteger(row.blockHeight) + '</a>';
          },
        },
        {
          id: "amount",
          header: "XEC amount",
          adjust: true,
          template: function (row) {
            return renderSats(row.satsAmount) + ' XEC';
          },
        },
      ],
      autoheight: true,
      autowidth: true,
      data: addrBalances[0].utxos,
    });
    isSatsTableLoaded = true;
  }
}

var isTokenTableLoaded = {};
function loadTokenTable(balanceIdx) {
  if (!isTokenTableLoaded[balanceIdx]) {
    console.log(addrBalances[balanceIdx].utxos);
    webix.ui({
      container: "tokens-coins-table-" + balanceIdx,
      view: "datatable",
      columns:[
        {
          id: "outpoint",
          header: "Outpoint",
          css: "hash",
          adjust: true,
          template: function (row) {
            return '<a href="/tx/' + row.txHash + '">' + 
              row.txHash + ':' + row.outIdx +
              (row.isCoinbase ? '<div class="ui green horizontal label">Coinbase</div>' : '') +
              '</a>';
          },
        },
        {
          id: "blockHeight",
          header: "Block Height",
          adjust: true,
          template: function (row) {
            return '<a href="/block-height/' + row.blockHeight + '">' + renderInteger(row.blockHeight) + '</a>';
          },
        },
        {
          id: "tokenAmount",
          header: addrBalances[balanceIdx].token.tokenTicker + " amount",
          adjust: true,
          template: function (row) {
            return renderAmount(row.tokenAmount, addrBalances[balanceIdx].token.decimals) + ' ' + addrBalances[balanceIdx].token.tokenTicker;
          },
        },
        {
          id: "satsAmount",
          header: "XEC amount",
          adjust: true,
          template: function (row) {
            return renderSats(row.satsAmount) + ' XEC';
          },
        },
      ],
      autoheight: true,
      autowidth: true,
      data: addrBalances[balanceIdx].utxos,
    });
    isTokenTableLoaded[balanceIdx] = true;
  }
}

const renderAge = timestamp => {
  if (timestamp == 0) {
    return '<div class="ui gray horizontal label">Mempool</div>';
  }
  return moment(timestamp * 1000).fromNow();
};

const renderTimestamp = timestamp => {
  if (timestamp == 0) {
    return '<div class="ui gray horizontal label">Mempool</div>';
  }
  return moment(timestamp * 1000).format('ll, LTS');
};

const renderTxID = txHash => {
  return '<a href="/tx/' + txHash + '">' + renderTxHash(txHash) + '</a>';
};

const renderBlockHeight = (_value, _type, row) => {
  if (row.timestamp == 0) {
    return '<div class="ui gray horizontal label">Mempool</div>';
  }
  return '<a href="/block-height/' + row.blockHeight + '">' + renderInteger(row.blockHeight) + '</a>';
};

const renderSize = size => formatByteSize(size);

const renderFee = (_value, _type, row) => {
  if (row.isCoinbase) {
    return '<div class="ui green horizontal label">Coinbase</div>';
  }

  const fee = renderInteger(row.satsInput - row.satsOutput);
  let markup = '';

  markup += `<span>${fee}</span>`
  markup += `<span class="fee-per-byte">&nbsp(${renderFeePerByte(_value, _type, row)})</span>`

  return markup;
};

const renderFeePerByte = (_value, _type, row) => {
  if (row.isCoinbase) {
    return '';
  }
  const fee = row.satsInput - row.satsOutput;
  const feePerByte = fee / row.size;
  return renderInteger(Math.round(feePerByte * 1000)) + '/kB';
};

const renderAmountXEC = deltaSats => renderSats(deltaSats) + ' XEC';

const renderToken = (_value, _type, row) => {
  if (row.token !== null) {
    var ticker = ' <a href="/tx/' + row.token.tokenId + '">' + row.token.tokenTicker + '</a>';
    return renderAmount(row.deltaTokens, row.token.decimals) + ticker;
  }
  return '';
};

const updateLoading = (status) => {
  if (status) {
    $('#address-txs-table > tbody').addClass('blur');
    $('#pagination').addClass('hidden');
    $('#footer').addClass('hidden');
  } else {
    $('#address-txs-table > tbody').removeClass('blur');
    $('#pagination').removeClass('hidden');
    $('#footer').removeClass('hidden');
  }
};

const datatable = () => {
  const address = getAddress();

  $('#address-txs-table').DataTable({
    searching: false,
    lengthMenu: [50, 100, 250, 500, 1000],
    pageLength: DEFAULT_ROWS_PER_PAGE,
    language: {
      loadingRecords: '',
      zeroRecords: '',
      emptyTable: '',
    },
    ajax: `/api/address/${address}/transactions`,
    order: [ [ 1, 'desc' ] ],
    responsive: {
        details: {
            type: 'column',
            target: -1
        }
    },
    columnDefs: [ {
        className: 'dtr-control',
        orderable: false,
        targets:   -1
    } ],
    columns:[
      { name: "age", data: 'timestamp', title: "Age", render: renderAge },
      { name: "timestamp", data: 'timestamp', title: "Date (UTC" + tzOffset + ")", render: renderTimestamp },
      { name: "txHash", data: 'txHash', title: "Transaction ID", className: "hash", render: renderTxID },
      { name: "blockHeight", title: "Block Height", render: renderBlockHeight },
      { name: "size", data: 'size', title: "Size", render: renderSize },
      { name: "fee", title: "Fee [sats]", className: "fee", render: renderFee },
      { name: "numInputs", data: 'numInputs', title: "Inputs" },
      { name: "numOutputs", data: 'numOutputs', title: "Outputs" },
      { name: "deltaSats", data: 'deltaSats', title: "Amount XEC", render: renderAmountXEC },
      { name: "token", title: "Amount Token", render: renderToken },
      { name: 'responsive', render: () => '' },
    ],
  });
}

$('#address-txs-table').on('xhr.dt', () => {
  updateLoading(false);
});

const updateTable = (paginationRequest) => {
  const params = new URLSearchParams(paginationRequest).toString();
  const address = getAddress();

  updateLoading(true);
  $('#address-txs-table').dataTable().api().ajax.url(`/api/address/${address}/transactions?${params}`).load()
}

const goToPage = (event, page) => {
  event.preventDefault();
  reRenderPage({ page });
};

$(document).on('change', '[name="address-txs-table_length"]', event => {
  reRenderPage({ rows: event.target.value, page: 1 });
});

const reRenderPage = params => {
  if (params) {
    window.state.updateParameters(params)
  }

  const paginationRequest = window.pagination.generatePaginationRequestOffset();
  updateTable(paginationRequest);

  const { currentPage, pageArray } = window.pagination.generatePaginationUIParams();
  window.pagination.generatePaginationUI(currentPage, pageArray);
};

$(document).ready(() => {
  datatable();
  reRenderPage();
});
