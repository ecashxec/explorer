const getAddress = () => window.location.pathname.split('/')[2];

const renderOutpoint = (_value, _type, row) => {
  const { txHash, outIdx } = row;
  const label = row.isCoinbase ? '<div class="ui green horizontal label">Coinbase</div>' : '';
  return `<a href="/tx/${txHash}">${txHash}:${outIdx}${label}</a>`;
};

const renderOutpointHeight = (_value, _type, row) => {
  const { blockHeight } = row;
  return `<a href="/block-height/${blockHeight}">${renderInteger(blockHeight)}</a>`;
};

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

const renderAmountXEC = sats => renderSats(sats) + ' XEC';

const renderTokenWithTicker = (_value, _type, row) => {
  if (row.token !== null) {
    var ticker = ' <a href="/tx/' + row.token.tokenId + '">' + row.token.tokenTicker + '</a>';
    return renderAmount(row.deltaTokens, row.token.decimals) + ticker;
  }
  return '';
};

const renderToken = (_value, _type, row) => renderAmount(row.tokenAmount, row.token.decimals);

const updateLoading = (status, tableId) => {
  if (status) {
    $(`#${tableId} > tbody`).addClass('blur');
    $('#pagination').addClass('hidden');
    $('#footer').addClass('hidden');
  } else {
    $(`#${tableId} > tbody`).removeClass('blur');
    $('#pagination').removeClass('hidden');
    $('#footer').removeClass('hidden');
  }
};

const datatableTxs = () => {
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
      { name: "token", title: "Amount Token", render: renderTokenWithTicker },
      { name: 'responsive', render: () => '' },
    ],
  });

  params = window.state.getParameters();
  $('#address-txs-table').dataTable().api().page.len(params.txRows);
}

const datatableCashOutpoints = cashOutpoints => {
  $('#outpoints-table').DataTable({
    searching: false,
    lengthMenu: [50, 100, 250, 500, 1000],
    pageLength: DEFAULT_ROWS_PER_PAGE,
    language: {
      loadingRecords: '',
      zeroRecords: '',
      emptyTable: '',
    },
    data: cashOutpoints,
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
      { name: "outpoint", className: "hash", render: renderOutpoint },
      { name: "block", render: renderOutpointHeight },
      { name: "xec", data: 'satsAmount', render: renderAmountXEC },
      { name: 'responsive', render: () => '' },
    ],
  });

  params = window.state.getParameters();
  $('#outpoints-table').dataTable().api().page.len(params.eCashOutpointsRows);
};

const datatableTokenOutpoints = tokenUtxos => {
  $('#address-token-outpoints-table').DataTable({
    searching: false,
    lengthMenu: [50, 100, 250, 500, 1000],
    pageLength: DEFAULT_ROWS_PER_PAGE,
    language: {
      loadingRecords: '',
      zeroRecords: '',
      emptyTable: '',
    },
    data: tokenUtxos,
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
      { name: 'outpoint', className: "hash", render: renderOutpoint },
      { name: 'block', render: renderOutpointHeight },
      { name: 'amount', data: 'tokenAmount', render: () => '' },
      { name: 'dust', data: 'satsAmount', render: renderAmountXEC },
      { name: 'responsive', render: () => '' },
    ],
  });
};

const datatableTokenBalances = tokenBalances => {
  $('#address-token-balances-table').DataTable({
    searching: false,
    lengthMenu: [50, 100, 250, 500, 1000],
    pageLength: DEFAULT_ROWS_PER_PAGE,
    language: {
      loadingRecords: '',
      zeroRecords: '',
      emptyTable: '',
    },
    data: tokenBalances,
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
      { name: 'amount', data: 'tokenAmount', render: renderToken },
      { name: 'ticker', data: 'token.tokenTicker' },
      { name: 'name', data: 'token.tokenName' },
      { name: 'dust', data: 'satsAmount', render: renderAmountXEC },
      { name: 'responsive', render: () => '' },
    ],
  });
};

$('#address-txs-table').on('xhr.dt', () => {
  updateLoading(false, 'address-txs-table');
});

$('#outpoints-table').on('xhr.dt', () => {
  updateLoading(false, 'outpoints-table');
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

$(document).on('change', '[name*="-table_length"]', event => {
  reRenderPage({
    rows: event.target.value,
    page: 1,
  });
});

const reRenderPage = params => {
  if (params) {
    params = window.state.updateParameters(params);
  } else {
    params = window.state.getParameters();

    if (!params.currentTab) {
      window.state.updateParameters({ currentTab: 'transactions' });
    }
  }

  if (params.currentTab) {
    $('.menu .item').tab('change tab', params.currentTab);
  }

  const paginationRequest = window.pagination.generatePaginationRequestOffset();
  updateTable(paginationRequest);

  const { currentPage, pageArray } = window.pagination.generatePaginationUIParams();
  window.pagination.generatePaginationUI(currentPage, pageArray);
};

$('#outpoints-table').on('init.dt', () => {
  updateLoading(false, 'outpoints-table');
});

$('#address-token-balances-table').on('init.dt', () => {
  updateLoading(false, 'address-token-balances-table');
});

$('#address-token-outpoints-table').on('init.dt', () => {
  updateLoading(false, 'address-token-outpoints-table');
});

const getAddressBalances = () => {
  const address = getAddress();
  return fetch(`/api/address/${address}/balances`)
    .then(response => response.json())
    .then(response => response.data);
}

$(document).ready(() => {
  datatableTxs();
  getAddressBalances()
    .then(balances => {
      const cashBalance = balances.shift();
      const tokenBalances = balances;

      let tokenUtxos = []
      if (tokenBalances.length > 0) { 
        tokenUtxos = tokenBalances.reduce((acc, balance) => (
          [].concat(acc, balance.utxos))
        );
      }

      datatableCashOutpoints(cashBalance.utxos);
      datatableTokenOutpoints(tokenUtxos);
      datatableTokenBalances(tokenBalances);
    });

  $('.menu .item').tab({
    onVisible: tabPath => (
      reRenderPage({ currentTab: tabPath })
    )
  });

  reRenderPage()
});
