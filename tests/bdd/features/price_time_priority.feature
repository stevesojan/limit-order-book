Feature: Price-Time Priority
  As an exchange matching engine
  I want strict price-time priority
  So that earlier orders at the same price execute first

  Scenario: Earlier order executes first
    Given a buy order of 100 shares at 101
    And another buy order of 100 shares at 101
    When a sell order of 150 shares at 101 arrives
    Then the first order is completely filled
    And the second order has 50 shares remaining

  Scenario: Match across price levels
    Given a sell order of 50 shares at 100
    And a sell order of 50 shares at 101
    When a buy order of 70 shares at 101 arrives
    Then 2 trades are generated
    And the best ask is 101 with 30 shares
