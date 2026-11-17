import 'package:material_ui/material_ui.dart';

import '../core/session_controller.dart';
import 'ajustes_page.dart';
import 'botoes_page.dart';
import 'orientacao_page.dart';
import 'painel_page.dart';

class XrLabApp extends StatelessWidget {
  const XrLabApp({super.key, required this.controller});
  final SessionController controller;

  @override
  Widget build(BuildContext context) {
    ThemeData theme(Brightness b) => ThemeData(
      colorScheme: ColorScheme.fromSeed(seedColor: const Color(0xFF00897B), brightness: b),
      useMaterial3: true,
    );
    return MaterialApp(
      title: 'xr-lab',
      debugShowCheckedModeBanner: false,
      theme: theme(Brightness.light),
      darkTheme: theme(Brightness.dark),
      home: HomeShell(controller: controller),
    );
  }
}

class HomeShell extends StatefulWidget {
  const HomeShell({super.key, required this.controller, this.initialIndex = 0});
  final SessionController controller;
  final int initialIndex;

  @override
  State<HomeShell> createState() => _HomeShellState();
}

class _HomeShellState extends State<HomeShell> {
  late int _index = widget.initialIndex;

  static const _dests = [
    (Icons.dashboard_outlined, Icons.dashboard, 'Painel'),
    (Icons.explore_outlined, Icons.explore, 'Orientação'),
    (Icons.radio_button_unchecked, Icons.radio_button_checked, 'Botões'),
    (Icons.tune_outlined, Icons.tune, 'Ajustes'),
  ];

  @override
  Widget build(BuildContext context) {
    final c = widget.controller;
    final pages = [
      PainelPage(controller: c),
      OrientacaoPage(controller: c),
      BotoesPage(controller: c),
      AjustesPage(controller: c),
    ];
    final appBar = AppBar(
      title: const Text('xr-lab'),
      actions: [
        ValueListenableBuilder<bool>(
          valueListenable: c.simulated,
          builder: (context, sim, _) => PopupMenuButton<bool>(
            key: const Key('fonte'),
            tooltip: 'Fonte dos dados',
            icon: Icon(sim ? Icons.science : Icons.usb),
            initialValue: sim,
            onSelected: (v) => c.start(simulated: v),
            itemBuilder: (_) => const [
              PopupMenuItem(value: false, child: Text('Óculos (USB)')),
              PopupMenuItem(value: true, child: Text('Simulado')),
            ],
          ),
        ),
      ],
    );
    final body = ValueListenableBuilder<String?>(
      valueListenable: c.error,
      builder: (context, err, child) => err == null ? child! : Center(child: Text(err)),
      child: pages[_index],
    );
    return LayoutBuilder(
      builder: (context, box) {
        if (box.maxWidth >= 720) {
          return Scaffold(
            appBar: appBar,
            body: Row(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                NavigationRail(
                  selectedIndex: _index,
                  onDestinationSelected: (i) => setState(() => _index = i),
                  labelType: NavigationRailLabelType.all,
                  destinations: [
                    for (final d in _dests)
                      NavigationRailDestination(icon: Icon(d.$1), selectedIcon: Icon(d.$2), label: Text(d.$3)),
                  ],
                ),
                const VerticalDivider(width: 1),
                Expanded(child: body),
              ],
            ),
          );
        }
        return Scaffold(
          appBar: appBar,
          body: body,
          bottomNavigationBar: NavigationBar(
            selectedIndex: _index,
            onDestinationSelected: (i) => setState(() => _index = i),
            destinations: [
              for (final d in _dests) NavigationDestination(icon: Icon(d.$1), selectedIcon: Icon(d.$2), label: d.$3),
            ],
          ),
        );
      },
    );
  }
}
